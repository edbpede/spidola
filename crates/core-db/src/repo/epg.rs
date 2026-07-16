// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Bounded EPG persistence and now/next queries (PRD §6.6).

use rusqlite::{Connection, Row, params};

use core_model::{ChannelIdentity, EpgEntry, EpgEntryId, SecretRef, SourceId};

use crate::error::{DbError, DbResult};
use crate::pool::Db;
use crate::repo::sources;

/// Terminal outcome of an atomic EPG staging swap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EpgCommit {
    /// The source still existed and its schedule was replaced.
    Committed { inserted: u64 },
    /// The source was removed while the schedule staged off-lock.
    SourceRemoved,
}

/// Writer-free, bounded EPG staging store. Dropping it leaves the live schedule untouched.
pub struct EpgStaging {
    conn: Connection,
    dir: tempfile::TempDir,
    source: SourceId,
}

const STAGING_DDL: &str = "\
CREATE TABLE _epg_staging (
    channel_identity INTEGER NOT NULL,
    title            TEXT NOT NULL,
    description      TEXT,
    start_unix       INTEGER NOT NULL,
    end_unix         INTEGER NOT NULL,
    CHECK(end_unix > start_unix),
    UNIQUE(channel_identity, start_unix, end_unix)
);";

impl Db {
    /// Opens a private temp-file EPG staging store without taking the live writer.
    ///
    /// # Errors
    /// Returns [`DbError`] when the temporary database cannot be created.
    #[allow(clippy::unused_self)]
    pub fn begin_epg_staging(&self, source: SourceId) -> DbResult<EpgStaging> {
        let dir = tempfile::TempDir::new().map_err(DbError::Staging)?;
        let conn =
            Connection::open(dir.path().join("epg-staging.sqlite")).map_err(DbError::Connection)?;
        conn.execute_batch(
            "PRAGMA journal_mode = MEMORY;
             PRAGMA synchronous = OFF;",
        )?;
        conn.execute_batch(STAGING_DDL)?;
        conn.execute_batch("BEGIN")?;
        Ok(EpgStaging { conn, dir, source })
    }
}

impl EpgStaging {
    /// Writes one parser-sized batch to the private staging file.
    ///
    /// # Errors
    /// Returns [`DbError`] if the batch cannot be staged.
    pub fn stage(&mut self, entries: &[EpgEntry]) -> DbResult<()> {
        let mut statement = self.conn.prepare_cached(
            "INSERT INTO _epg_staging(\
             channel_identity, title, description, start_unix, end_unix) \
             VALUES (?1, ?2, ?3, ?4, ?5) \
             ON CONFLICT(channel_identity, start_unix, end_unix) DO UPDATE SET \
             title = excluded.title, description = excluded.description",
        )?;
        for entry in entries
            .iter()
            .filter(|entry| entry.source_id == self.source)
        {
            statement.execute(params![
                entry.channel.to_storage(),
                entry.title,
                entry.description,
                entry.start_unix,
                entry.end_unix,
            ])?;
        }
        Ok(())
    }

    /// Atomically swaps the staged schedule into the live database.
    ///
    /// # Errors
    /// Returns [`DbError`] if the staging transaction or live swap fails.
    pub fn commit(self, db: &Db) -> DbResult<EpgCommit> {
        self.conn.execute_batch("COMMIT")?;
        let guard = db.writer();
        let path = self.dir.path().join("epg-staging.sqlite");
        let path = path.to_string_lossy();
        guard.execute("ATTACH DATABASE ?1 AS epg_stg", params![path.as_ref()])?;
        let result = self.swap_under_writer(&guard);
        if result.is_err() {
            let _ = guard.execute_batch("ROLLBACK");
        }
        let _ = guard.execute_batch("DETACH DATABASE epg_stg");
        result
    }

    fn swap_under_writer(&self, conn: &Connection) -> DbResult<EpgCommit> {
        conn.execute_batch("BEGIN IMMEDIATE")?;
        if !sources::exists(conn, self.source)? {
            conn.execute_batch("ROLLBACK")?;
            return Ok(EpgCommit::SourceRemoved);
        }
        conn.execute(
            "DELETE FROM epg_entries WHERE source_id = ?1",
            params![self.source.value()],
        )?;
        conn.execute(
            "INSERT INTO epg_entries(\
             source_id, channel_identity, title, description, start_unix, end_unix) \
             SELECT ?1, channel_identity, title, description, start_unix, end_unix \
             FROM epg_stg._epg_staging",
            params![self.source.value()],
        )?;
        let inserted = conn.changes();
        conn.execute_batch("COMMIT")?;
        Ok(EpgCommit::Committed { inserted })
    }
}

/// Stores the opaque secure-store reference for a source's XMLTV feed.
///
/// # Errors
/// Returns [`DbError`] on a write failure.
pub fn set_feed(conn: &Connection, source: SourceId, secret_ref: &SecretRef) -> DbResult<()> {
    conn.execute(
        "INSERT INTO epg_feeds(source_id, secret_ref) VALUES (?1, ?2) \
         ON CONFLICT(source_id) DO UPDATE SET secret_ref = excluded.secret_ref",
        params![source.value(), secret_ref.as_str()],
    )?;
    Ok(())
}

/// Returns the opaque secure-store reference for a source's XMLTV feed.
///
/// # Errors
/// Returns [`DbError`] on a query failure.
pub fn get_feed(conn: &Connection, source: SourceId) -> DbResult<Option<SecretRef>> {
    let mut statement = conn.prepare("SELECT secret_ref FROM epg_feeds WHERE source_id = ?1")?;
    let mut rows = statement.query(params![source.value()])?;
    Ok(rows
        .next()?
        .map(|row| row.get::<_, String>(0).map(SecretRef::new))
        .transpose()?)
}

/// Removes a source's configured feed, returning its secure-store reference for cleanup.
///
/// # Errors
/// Returns [`DbError`] on a query or write failure.
pub fn remove_feed(conn: &Connection, source: SourceId) -> DbResult<Option<SecretRef>> {
    let feed = get_feed(conn, source)?;
    conn.execute(
        "DELETE FROM epg_feeds WHERE source_id = ?1",
        params![source.value()],
    )?;
    Ok(feed)
}

/// Replaces one source's rolling EPG window atomically.
///
/// # Errors
/// Returns [`DbError`](crate::error::DbError) on a query failure.
pub fn replace_window(
    conn: &mut Connection,
    source: SourceId,
    earliest_unix: i64,
    latest_unix: i64,
    entries: &[EpgEntry],
) -> DbResult<u64> {
    let transaction = conn.transaction()?;
    transaction.execute(
        "DELETE FROM epg_entries WHERE source_id = ?1",
        params![source.value()],
    )?;
    let mut insert = transaction.prepare(
        "INSERT INTO epg_entries(\
         source_id, channel_identity, title, description, start_unix, end_unix) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
         ON CONFLICT(source_id, channel_identity, start_unix, end_unix) DO UPDATE SET \
         title = excluded.title, description = excluded.description",
    )?;
    let mut written = 0_u64;
    for entry in entries.iter().filter(|entry| {
        entry.source_id == source
            && entry.end_unix > earliest_unix
            && entry.start_unix < latest_unix
            && entry.end_unix > entry.start_unix
    }) {
        insert.execute(params![
            source.value(),
            entry.channel.to_storage(),
            entry.title,
            entry.description,
            entry.start_unix,
            entry.end_unix,
        ])?;
        written = written.saturating_add(1);
    }
    drop(insert);
    transaction.commit()?;
    Ok(written)
}

/// Removes entries wholly outside the configured rolling window.
///
/// # Errors
/// Returns [`DbError`](crate::error::DbError) on a write failure.
pub fn prune(conn: &Connection, earliest_unix: i64, latest_unix: i64) -> DbResult<u64> {
    let count = conn.execute(
        "DELETE FROM epg_entries WHERE end_unix <= ?1 OR start_unix >= ?2",
        params![earliest_unix, latest_unix],
    )?;
    Ok(u64::try_from(count).unwrap_or(u64::MAX))
}

/// Returns the current and next programme for one channel.
///
/// # Errors
/// Returns [`DbError`](crate::error::DbError) on a query failure.
pub fn now_next(
    conn: &Connection,
    source: SourceId,
    channel: ChannelIdentity,
    now_unix: i64,
) -> DbResult<(Option<EpgEntry>, Option<EpgEntry>)> {
    let mut statement = conn.prepare(
        "SELECT id, source_id, channel_identity, title, description, start_unix, end_unix \
         FROM epg_entries \
         WHERE source_id = ?1 AND channel_identity = ?2 AND end_unix > ?3 \
         ORDER BY start_unix LIMIT 2",
    )?;
    let mut rows = statement.query(params![source.value(), channel.to_storage(), now_unix])?;
    let first = rows.next()?.map(map_entry).transpose()?;
    let second = rows.next()?.map(map_entry).transpose()?;
    match first {
        Some(entry) if entry.is_current(now_unix) => Ok((Some(entry), second)),
        Some(entry) => Ok((None, Some(entry))),
        None => Ok((None, None)),
    }
}

/// Lists a bounded page intersecting a time window.
///
/// # Errors
/// Returns [`DbError`](crate::error::DbError) on a query failure.
pub fn list_window(
    conn: &Connection,
    source: SourceId,
    channel: ChannelIdentity,
    earliest_unix: i64,
    latest_unix: i64,
    offset: u32,
    limit: u32,
) -> DbResult<Vec<EpgEntry>> {
    let mut statement = conn.prepare(
        "SELECT id, source_id, channel_identity, title, description, start_unix, end_unix \
         FROM epg_entries WHERE source_id = ?1 AND channel_identity = ?2 \
         AND end_unix > ?3 AND start_unix < ?4 \
         ORDER BY start_unix LIMIT ?5 OFFSET ?6",
    )?;
    let rows = statement.query_map(
        params![
            source.value(),
            channel.to_storage(),
            earliest_unix,
            latest_unix,
            limit,
            offset
        ],
        map_entry,
    )?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

fn map_entry(row: &Row<'_>) -> rusqlite::Result<EpgEntry> {
    Ok(EpgEntry {
        id: EpgEntryId::new(row.get("id")?),
        source_id: SourceId::new(row.get("source_id")?),
        channel: ChannelIdentity::from_storage(row.get("channel_identity")?),
        title: row.get("title")?,
        description: row.get("description")?,
        start_unix: row.get("start_unix")?,
        end_unix: row.get("end_unix")?,
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::pool::Db;

    fn entry(start: i64, end: i64, title: &str) -> EpgEntry {
        EpgEntry {
            id: EpgEntryId::new(0),
            source_id: SourceId::new(1),
            channel: ChannelIdentity::from_raw(7),
            title: title.to_owned(),
            description: None,
            start_unix: start,
            end_unix: end,
        }
    }

    #[test]
    fn replacement_is_windowed_and_now_next_is_stable() {
        let db = Db::open_in_memory().unwrap();
        let mut conn = db.writer();
        conn.execute(
            "INSERT INTO sources(id, kind, name) VALUES (1, 'm3u-url', 'Source')",
            [],
        )
        .unwrap();
        let entries = vec![
            entry(0, 50, "old"),
            entry(90, 110, "now"),
            entry(110, 130, "next"),
        ];
        assert_eq!(
            replace_window(&mut conn, SourceId::new(1), 80, 140, &entries).unwrap(),
            2
        );
        let (current, next) =
            now_next(&conn, SourceId::new(1), ChannelIdentity::from_raw(7), 100).unwrap();
        assert_eq!(current.unwrap().title, "now");
        assert_eq!(next.unwrap().title, "next");
    }
}
