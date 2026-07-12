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
}
