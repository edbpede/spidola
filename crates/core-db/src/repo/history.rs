// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Playback-history repository (TECH_SPEC §4.4, PRD §6.5). Snapshots name + locator at
//! play time and keys the channel by stable identity, so a "recently watched" row stays
//! replayable across refreshes. Purge and off-switch are user-facing (settings).

use rusqlite::{Connection, params};

use core_model::history::PlaybackHistoryEntry;
use core_model::ids::{ChannelIdentity, HistoryId, SourceId};
use core_model::locator::StreamLocator;

use crate::error::{DbError, DbResult};

/// A history record to store — everything but the assigned id.
#[derive(Debug, Clone)]
pub struct NewHistory {
    /// Owning source.
    pub source_id: SourceId,
    /// Stable identity of the played channel.
    pub identity: ChannelIdentity,
    /// Name at play time.
    pub name: String,
    /// Locator at play time.
    pub locator: StreamLocator,
    /// When it was played, Unix seconds (injected by the caller).
    pub played_at_unix: i64,
    /// Resume position in seconds, if recorded.
    pub position_secs: Option<u32>,
}

/// Records a playback event, returning its id.
///
/// # Errors
/// Returns [`DbError`] on a write failure.
pub fn record(conn: &Connection, entry: &NewHistory) -> DbResult<HistoryId> {
    conn.execute(
        "INSERT INTO playback_history(source_id, identity, name, locator, \
         played_at_unix, position_secs) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            entry.source_id.value(),
            entry.identity.to_storage(),
            entry.name,
            entry.locator.as_str(),
            entry.played_at_unix,
            entry.position_secs,
        ],
    )?;
    Ok(HistoryId::new(conn.last_insert_rowid()))
}

/// Returns the most recent history entries, newest first, capped at `limit`.
///
/// # Errors
/// Returns [`DbError`] on a query or mapping failure.
pub fn recent(conn: &Connection, limit: u32) -> DbResult<Vec<PlaybackHistoryEntry>> {
    let mut stmt = conn.prepare(
        "SELECT id, source_id, identity, name, locator, played_at_unix, position_secs \
         FROM playback_history ORDER BY played_at_unix DESC, id DESC LIMIT ?1",
    )?;
    let mut rows = stmt.query(params![limit])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        let locator_raw: String = row.get("locator")?;
        let locator = StreamLocator::parse(&locator_raw)
            .map_err(|e| DbError::Integrity(format!("stored locator is invalid: {e}")))?;
        out.push(PlaybackHistoryEntry {
            id: HistoryId::new(row.get("id")?),
            source_id: SourceId::new(row.get("source_id")?),
            identity: ChannelIdentity::from_storage(row.get("identity")?),
            name: row.get("name")?,
            locator,
            played_at_unix: row.get("played_at_unix")?,
            position_secs: row
                .get::<_, Option<i64>>("position_secs")?
                .and_then(|v| u32::try_from(v).ok()),
        });
    }
    Ok(out)
}

/// Clears all playback history (the one-toggle purge, PRD §6.5).
///
/// # Errors
/// Returns [`DbError`] on a write failure.
pub fn clear(conn: &Connection) -> DbResult<()> {
    conn.execute("DELETE FROM playback_history", [])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::pool::Db;

    #[test]
    fn record_recent_clear() {
        let db = Db::open_in_memory().unwrap();
        let conn = db.writer();
        conn.execute(
            "INSERT INTO sources(id, kind, name) VALUES (1, 'm3u-url', 'S')",
            [],
        )
        .unwrap();
        let entry = NewHistory {
            source_id: SourceId::new(1),
            identity: ChannelIdentity::from_raw(7),
            name: "BBC One".to_owned(),
            locator: StreamLocator::parse("http://x/1").unwrap(),
            played_at_unix: 1000,
            position_secs: Some(42),
        };
        record(&conn, &entry).unwrap();
        let recents = recent(&conn, 10).unwrap();
        assert_eq!(recents.len(), 1);
        assert_eq!(recents[0].position_secs, Some(42));
        clear(&conn).unwrap();
        assert!(recent(&conn, 10).unwrap().is_empty());
    }
}
