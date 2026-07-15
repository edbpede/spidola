// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Forward-only, numbered migrations (rusqlite_migration).
//!
//! Migrations are applied at startup and only ever move forward; a downgraded app that
//! meets a newer `user_version` refuses rather than guessing (enforced by `core-api`'s
//! handshake, TECH_SPEC §13). Never edit a shipped migration — add the next number. The
//! migration test harness upgrades every historical schema to head (§10).

use std::sync::LazyLock;

use rusqlite::Connection;
use rusqlite_migration::{M, Migrations};

use crate::error::DbResult;

/// The current schema version — the boundary handshake reports this (TECH_SPEC §13).
pub const SCHEMA_VERSION: usize = 2;

/// Migration 001: the full Phase-1 schema.
///
/// Notes: tables are `STRICT` so type affinity can't paper over bugs; `channels` carries a
/// generated `is_live` column for the hot browse filter (§4.4); secrets never appear —
/// Xtream stores `username` + an opaque `secret_ref` only (§12); the FTS5 index is
/// contentless with `contentless_delete=1` and kept in sync by triggers.
const M001_INIT: &str = "\
CREATE TABLE sources (
    id                 INTEGER PRIMARY KEY,
    kind               TEXT    NOT NULL,
    name               TEXT    NOT NULL,
    enabled            INTEGER NOT NULL DEFAULT 1,
    auto_refresh_secs  INTEGER,
    url                TEXT,
    username           TEXT,
    secret_ref         TEXT,
    user_agent         TEXT,
    accept_invalid_tls INTEGER NOT NULL DEFAULT 0,
    created_at_unix    INTEGER NOT NULL DEFAULT 0
) STRICT;

CREATE TABLE categories (
    id        INTEGER PRIMARY KEY,
    source_id INTEGER NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
    kind      TEXT    NOT NULL,
    name      TEXT    NOT NULL,
    remote_id TEXT,
    UNIQUE(source_id, kind, name)
) STRICT;

CREATE TABLE channels (
    id               INTEGER PRIMARY KEY,
    source_id        INTEGER NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
    identity         INTEGER NOT NULL,
    name             TEXT    NOT NULL,
    group_title      TEXT,
    logo             TEXT,
    locator          TEXT    NOT NULL,
    kind             TEXT    NOT NULL,
    category_id      INTEGER REFERENCES categories(id) ON DELETE SET NULL,
    user_agent       TEXT,
    headers          TEXT,
    preferred_engine TEXT,
    sort_index       INTEGER NOT NULL DEFAULT 0,
    is_live          INTEGER GENERATED ALWAYS AS (kind = 'live') VIRTUAL,
    UNIQUE(source_id, identity)
) STRICT;

CREATE INDEX idx_channels_source   ON channels(source_id, sort_index);
CREATE INDEX idx_channels_category ON channels(category_id);
CREATE INDEX idx_channels_live     ON channels(source_id, is_live);

CREATE TABLE favorites (
    source_id       INTEGER NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
    identity        INTEGER NOT NULL,
    created_at_unix INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (source_id, identity)
) STRICT;

CREATE TABLE hidden_channels (
    source_id INTEGER NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
    identity  INTEGER NOT NULL,
    PRIMARY KEY (source_id, identity)
) STRICT;

CREATE TABLE playback_history (
    id             INTEGER PRIMARY KEY,
    source_id      INTEGER NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
    identity       INTEGER NOT NULL,
    name           TEXT    NOT NULL,
    locator        TEXT    NOT NULL,
    played_at_unix INTEGER NOT NULL,
    position_secs  INTEGER
) STRICT;

CREATE INDEX idx_history_played ON playback_history(played_at_unix DESC);

CREATE TABLE settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
) STRICT;

CREATE TABLE import_diagnostics (
    source_id         INTEGER PRIMARY KEY REFERENCES sources(id) ON DELETE CASCADE,
    refreshed_at_unix INTEGER NOT NULL,
    total             INTEGER NOT NULL,
    skipped           INTEGER NOT NULL
) STRICT;

CREATE VIRTUAL TABLE channel_search USING fts5(
    name,
    group_title,
    content='',
    contentless_delete=1,
    tokenize='unicode61 remove_diacritics 2'
);

CREATE TRIGGER channels_ai AFTER INSERT ON channels BEGIN
    INSERT INTO channel_search(rowid, name, group_title)
    VALUES (new.id, new.name, coalesce(new.group_title, ''));
END;

CREATE TRIGGER channels_ad AFTER DELETE ON channels BEGIN
    DELETE FROM channel_search WHERE rowid = old.id;
END;

CREATE TRIGGER channels_au AFTER UPDATE ON channels BEGIN
    DELETE FROM channel_search WHERE rowid = old.id;
    INSERT INTO channel_search(rowid, name, group_title)
    VALUES (new.id, new.name, coalesce(new.group_title, ''));
END;
";

/// Migration 002: credential-bearing M3U values move out of plaintext SQLite.
///
/// Pre-1.0 data compatibility is deliberately not owed (TECH_SPEC §13). Existing M3U sources are
/// removed rather than pretending an in-place UPDATE can erase their URLs from WAL/freelist
/// pages; `Db::open` performs the post-migration checkpoint + vacuum that completes the cutover.
const M002_SEALED_M3U: &str = "\
ALTER TABLE sources ADD COLUMN has_user_agent INTEGER NOT NULL DEFAULT 0;
DELETE FROM sources WHERE kind IN ('m3u-url', 'm3u-file');
INSERT INTO settings(key, value) VALUES ('security.m3u_scrub.v2', 'pending')
ON CONFLICT(key) DO UPDATE SET value = 'pending';
";

/// The ordered migration set. Append the next `M::up(...)` to grow the schema.
static MIGRATIONS: LazyLock<Migrations<'static>> =
    LazyLock::new(|| Migrations::new(vec![M::up(M001_INIT), M::up(M002_SEALED_M3U)]));

/// Applies all pending migrations, bringing `conn` to [`SCHEMA_VERSION`].
///
/// # Errors
/// Returns [`DbError::Migration`](crate::error::DbError::Migration) if any step fails.
pub fn apply(conn: &mut Connection) -> DbResult<()> {
    MIGRATIONS.to_latest(conn)?;
    Ok(())
}

/// Applies migrations up to a specific version — used only by the migration test harness
/// to reconstruct historical schemas.
#[cfg(test)]
pub(crate) fn apply_to_version(conn: &mut Connection, version: usize) -> DbResult<()> {
    MIGRATIONS.to_version(conn, version)?;
    Ok(())
}

/// Validates the migration set (no gaps, well-formed SQL structure).
///
/// # Errors
/// Returns [`DbError::Migration`](crate::error::DbError::Migration) on a malformed set.
pub fn validate() -> DbResult<()> {
    MIGRATIONS.validate()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    fn user_version(conn: &Connection) -> usize {
        conn.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .map(|v| usize::try_from(v).unwrap_or(0))
            .unwrap()
    }

    #[test]
    fn migration_set_is_valid() {
        validate().unwrap();
    }

    #[test]
    fn fresh_db_reaches_head() {
        let mut conn = Connection::open_in_memory().unwrap();
        apply(&mut conn).unwrap();
        assert_eq!(user_version(&conn), SCHEMA_VERSION);
    }

    #[test]
    fn apply_is_idempotent() {
        let mut conn = Connection::open_in_memory().unwrap();
        apply(&mut conn).unwrap();
        // A second application is a no-op and must not error.
        apply(&mut conn).unwrap();
        assert_eq!(user_version(&conn), SCHEMA_VERSION);
    }

    #[test]
    fn every_historical_schema_upgrades_to_head() {
        // For each intermediate version, build that schema then upgrade to head.
        for start in 0..=SCHEMA_VERSION {
            let mut conn = Connection::open_in_memory().unwrap();
            apply_to_version(&mut conn, start).unwrap();
            assert_eq!(user_version(&conn), start);
            apply(&mut conn).unwrap();
            assert_eq!(user_version(&conn), SCHEMA_VERSION);
            // Head schema is smoke-queryable.
            let n: i64 = conn
                .query_row("SELECT count(*) FROM channels", [], |r| r.get(0))
                .unwrap();
            assert_eq!(n, 0);
        }
    }
}
