// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! FTS5 search index configuration and trigger-driven maintenance (TECH_SPEC §4.4).
//!
//! The index is a **contentless** FTS5 table (`content=''`) with `contentless_delete=1`,
//! so it stores only the inverted index — never a second copy of the channel rows — and
//! still supports deletes/updates via the `'delete'` command. Its `rowid` is the
//! `channels.id`, so a search matches to a rowid and the query layer (`core-search`) joins
//! back to `channels` for the row data. The DDL and triggers live in migration 001
//! ([`crate::migrations`]); this module owns the runtime maintenance operations.

use rusqlite::Connection;

use crate::error::DbResult;

/// The FTS5 virtual table name.
pub const TABLE: &str = "channel_search";

/// Runs FTS5 `optimize`, merging b-tree segments after a large refresh so subsequent
/// queries stay on the sub-50 ms budget (PRD §9). Blocking; call from the service layer's
/// blocking adapter only.
pub(crate) fn optimize(conn: &Connection) -> DbResult<()> {
    conn.execute(
        "INSERT INTO channel_search(channel_search) VALUES('optimize')",
        [],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::migrations;

    #[test]
    fn optimize_runs_and_index_matches() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrations::apply(&mut conn).unwrap();
        conn.execute(
            "INSERT INTO sources(id, kind, name) VALUES (1, 'm3u-url', 'S')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO channels(id, source_id, identity, name, locator, kind) \
             VALUES (1, 1, 10, 'BBC One', 'http://x', 'live')",
            [],
        )
        .unwrap();
        optimize(&conn).unwrap();
        // The trigger fed the contentless index; a MATCH resolves back to the rowid.
        let id: i64 = conn
            .query_row(
                "SELECT rowid FROM channel_search WHERE channel_search MATCH 'bbc'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(id, 1);
    }
}
