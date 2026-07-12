// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Favorites repository (TECH_SPEC §4.4, PRD §6.5). Keyed by `(source, stable identity)`
//! so favorites survive a catalog refresh that renumbers every channel. Times are Unix
//! seconds injected by the caller — this layer has no clock.

use rusqlite::{Connection, params};

use core_model::favorite::Favorite;
use core_model::ids::{ChannelIdentity, SourceId};

use crate::error::DbResult;

/// Marks a channel favorite (idempotent).
///
/// # Errors
/// Returns [`DbError`](crate::error::DbError) on a write failure.
pub fn add(
    conn: &Connection,
    source: SourceId,
    identity: ChannelIdentity,
    created_at_unix: i64,
) -> DbResult<()> {
    conn.execute(
        "INSERT INTO favorites(source_id, identity, created_at_unix) VALUES (?1, ?2, ?3) \
         ON CONFLICT(source_id, identity) DO NOTHING",
        params![source.value(), identity.to_storage(), created_at_unix],
    )?;
    Ok(())
}

/// Removes a favorite (idempotent).
///
/// # Errors
/// Returns [`DbError`](crate::error::DbError) on a write failure.
pub fn remove(conn: &Connection, source: SourceId, identity: ChannelIdentity) -> DbResult<()> {
    conn.execute(
        "DELETE FROM favorites WHERE source_id = ?1 AND identity = ?2",
        params![source.value(), identity.to_storage()],
    )?;
    Ok(())
}

/// Whether a channel is favorited.
///
/// # Errors
/// Returns [`DbError`](crate::error::DbError) on a query failure.
pub fn is_favorite(
    conn: &Connection,
    source: SourceId,
    identity: ChannelIdentity,
) -> DbResult<bool> {
    let n: i64 = conn.query_row(
        "SELECT count(*) FROM favorites WHERE source_id = ?1 AND identity = ?2",
        params![source.value(), identity.to_storage()],
        |r| r.get(0),
    )?;
    Ok(n > 0)
}

/// Lists a source's favorites, most recently added first.
///
/// # Errors
/// Returns [`DbError`](crate::error::DbError) on a query failure.
pub fn list_for_source(conn: &Connection, source: SourceId) -> DbResult<Vec<Favorite>> {
    let mut stmt = conn.prepare(
        "SELECT source_id, identity, created_at_unix FROM favorites \
         WHERE source_id = ?1 ORDER BY created_at_unix DESC",
    )?;
    let mut rows = stmt.query(params![source.value()])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(Favorite {
            source_id: SourceId::new(row.get("source_id")?),
            identity: ChannelIdentity::from_storage(row.get("identity")?),
            created_at_unix: row.get("created_at_unix")?,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::pool::Db;

    fn seed_source(conn: &Connection) -> SourceId {
        conn.execute(
            "INSERT INTO sources(id, kind, name) VALUES (1, 'm3u-url', 'S')",
            [],
        )
        .unwrap();
        SourceId::new(1)
    }

    #[test]
    fn add_is_idempotent_and_removable() {
        let db = Db::open_in_memory().unwrap();
        let conn = db.writer();
        let src = seed_source(&conn);
        let ident = ChannelIdentity::from_raw(99);
        add(&conn, src, ident, 100).unwrap();
        add(&conn, src, ident, 200).unwrap(); // no-op, keeps original timestamp
        assert!(is_favorite(&conn, src, ident).unwrap());
        assert_eq!(list_for_source(&conn, src).unwrap().len(), 1);
        remove(&conn, src, ident).unwrap();
        assert!(!is_favorite(&conn, src, ident).unwrap());
    }
}
