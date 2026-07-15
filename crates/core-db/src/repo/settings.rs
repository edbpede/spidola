// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Settings repository (TECH_SPEC §4.4, PRD §6.9). A flat key→value store; the typed
//! settings surface and defaults live in `core-api`'s `SettingsService` (Phase 2). Keys
//! are opaque strings here — no secret ever lands in this table (§12).

use rusqlite::{Connection, params};

use crate::error::DbResult;

/// Reads a setting value, if present.
///
/// # Errors
/// Returns [`DbError`](crate::error::DbError) on a query failure.
pub fn get(conn: &Connection, key: &str) -> DbResult<Option<String>> {
    let mut stmt = conn.prepare("SELECT value FROM settings WHERE key = ?1")?;
    let mut rows = stmt.query(params![key])?;
    match rows.next()? {
        Some(row) => Ok(Some(row.get(0)?)),
        None => Ok(None),
    }
}

/// Writes (upserts) a setting value.
///
/// # Errors
/// Returns [`DbError`](crate::error::DbError) on a write failure.
pub fn set(conn: &Connection, key: &str, value: &str) -> DbResult<()> {
    conn.execute(
        "INSERT INTO settings(key, value) VALUES (?1, ?2) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

/// Writes (upserts) two settings as one act: either both land or neither does.
///
/// Exists because not every setting is one row. The EPG window is a single choice the user makes
/// once and reads back as a pair (PRD §6.6), and two separate [`set`] calls cannot promise that
/// what comes back is a pair they ever chose — a fault between them leaves one new bound beside
/// one old one, a window nobody asked for and nothing later corrects. One transaction has no
/// between.
///
/// Each argument is a `(key, value)` pair, written in the order given.
///
/// # Errors
/// Returns [`DbError`](crate::error::DbError) on a write failure, with neither key changed.
pub fn set_pair(conn: &Connection, first: (&str, &str), second: (&str, &str)) -> DbResult<()> {
    conn.execute_batch("BEGIN IMMEDIATE")?;
    let written = set(conn, first.0, first.1).and_then(|()| set(conn, second.0, second.1));
    // Every failure below leaves the transaction open, and this connection is the writer every
    // later call borrows — so an unresolved one would not just fail this write, it would refuse
    // the next `BEGIN` on a handle nothing else can reset. Resolving it here is what keeps a
    // refused pair a refused pair rather than a wedged database.
    if let Err(error) = written.and_then(|()| Ok(conn.execute_batch("COMMIT")?)) {
        // Rolling back a failed `COMMIT` is SQLite's own prescription, not a guess: the
        // transaction stays live and awaits either a retry or this. The rollback's own result is
        // dropped because the error worth reporting is the one that got us here.
        let _ = conn.execute_batch("ROLLBACK");
        return Err(error);
    }
    Ok(())
}

/// Removes a setting, reverting it to its code default.
///
/// # Errors
/// Returns [`DbError`](crate::error::DbError) on a write failure.
pub fn remove(conn: &Connection, key: &str) -> DbResult<()> {
    conn.execute("DELETE FROM settings WHERE key = ?1", params![key])?;
    Ok(())
}

/// Returns every stored setting as `(key, value)` pairs.
///
/// # Errors
/// Returns [`DbError`](crate::error::DbError) on a query failure.
pub fn all(conn: &Connection) -> DbResult<Vec<(String, String)>> {
    let mut stmt = conn.prepare("SELECT key, value FROM settings ORDER BY key")?;
    let mut rows = stmt.query([])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push((row.get(0)?, row.get(1)?));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::pool::Db;

    #[test]
    fn upsert_get_remove() {
        let db = Db::open_in_memory().unwrap();
        let conn = db.writer();
        assert!(get(&conn, "ui.language").unwrap().is_none());
        set(&conn, "ui.language", "en").unwrap();
        set(&conn, "ui.language", "da").unwrap(); // upsert
        assert_eq!(get(&conn, "ui.language").unwrap().as_deref(), Some("da"));
        assert_eq!(all(&conn).unwrap().len(), 1);
        remove(&conn, "ui.language").unwrap();
        assert!(get(&conn, "ui.language").unwrap().is_none());
    }

    #[test]
    fn a_pair_writes_both_keys() {
        let db = Db::open_in_memory().unwrap();
        let conn = db.writer();
        set_pair(&conn, ("epg.ahead", "72"), ("epg.behind", "24")).unwrap();
        assert_eq!(get(&conn, "epg.ahead").unwrap().as_deref(), Some("72"));
        assert_eq!(get(&conn, "epg.behind").unwrap().as_deref(), Some("24"));
    }

    #[test]
    fn a_pair_refused_halfway_moves_neither_key() {
        // The whole reason `set_pair` exists, so it is the case worth pinning: the first write
        // lands, the second is refused, and what the user reads back afterwards must be the
        // window they already had rather than half of the one they asked for. The trigger stands
        // in for whatever might refuse the write — the point is only that something can.
        let db = Db::open_in_memory().unwrap();
        let conn = db.writer();
        set(&conn, "epg.ahead", "72").unwrap();
        conn.execute_batch(
            "CREATE TRIGGER refuse_behind BEFORE INSERT ON settings \
             WHEN NEW.key = 'epg.behind' \
             BEGIN SELECT RAISE(ABORT, 'the store said no'); END",
        )
        .unwrap();

        let refused = set_pair(&conn, ("epg.ahead", "24"), ("epg.behind", "12"));

        assert!(refused.is_err(), "a refused half must fail the whole pair");
        assert_eq!(
            get(&conn, "epg.ahead").unwrap().as_deref(),
            Some("72"),
            "the bound that did write must have gone back with the one that didn't"
        );
        assert!(
            get(&conn, "epg.behind").unwrap().is_none(),
            "the refused bound must not have landed"
        );
    }
}
