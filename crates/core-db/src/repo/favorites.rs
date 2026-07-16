// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Favorites repository (TECH_SPEC §4.4, PRD §6.5). Keyed by `(source, stable identity)`
//! so favorites survive a catalog refresh that renumbers every channel. Times are Unix
//! seconds injected by the caller — this layer has no clock.

use rusqlite::{Connection, params};

use core_model::channel::Channel;
use core_model::favorite::Favorite;
use core_model::ids::{ChannelIdentity, SourceId};

use crate::error::DbResult;
use crate::repo::channels::{map_channel, prefixed_select_columns};

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
        "INSERT OR IGNORE INTO favorites(source_id, identity, created_at_unix, position) \
         SELECT ?1, ?2, ?3, coalesce(max(position) + 1, 0) FROM favorites",
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
        "SELECT source_id, identity, created_at_unix, position FROM favorites \
         WHERE source_id = ?1 ORDER BY position, created_at_unix DESC",
    )?;
    let mut rows = stmt.query(params![source.value()])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(Favorite {
            source_id: SourceId::new(row.get("source_id")?),
            identity: ChannelIdentity::from_storage(row.get("identity")?),
            created_at_unix: row.get("created_at_unix")?,
            position: row.get("position")?,
        });
    }
    Ok(out)
}

/// Counts favorites that resolve to a channel currently in an **enabled** source's catalog
/// (paging total for [`list_channels`]). Favorites whose channel is absent or whose source is
/// disabled are excluded, matching what the home row can actually show.
///
/// # Errors
/// Returns [`DbError`](crate::error::DbError) on a query failure.
pub fn count_channels(conn: &Connection) -> DbResult<u64> {
    let n: i64 = conn.query_row(
        "SELECT count(*) FROM favorites f \
         JOIN channels c ON c.source_id = f.source_id AND c.identity = f.identity \
         JOIN sources s ON s.id = f.source_id \
         WHERE s.enabled = 1",
        [],
        |r| r.get(0),
    )?;
    Ok(u64::try_from(n).unwrap_or(0))
}

/// Lists a page of favorited channels across all enabled sources, most recently favorited
/// first (the home "Favorites" row, PRD §8.3). Resolves each favorite to the channel in the
/// current catalog by stable identity; favorites with no matching channel are skipped (they
/// return if the channel reappears under the same identity). Paged by contract (§4.6).
///
/// # Errors
/// Returns [`DbError`](crate::error::DbError) on a query or row-mapping failure.
pub fn list_channels(conn: &Connection, offset: u32, limit: u32) -> DbResult<Vec<Channel>> {
    let columns = prefixed_select_columns("c");
    let sql = format!(
        "SELECT {columns} FROM favorites f \
         JOIN channels c ON c.source_id = f.source_id AND c.identity = f.identity \
         JOIN sources s ON s.id = f.source_id \
         WHERE s.enabled = 1 \
         ORDER BY f.position, f.created_at_unix DESC, c.id DESC \
         LIMIT ?1 OFFSET ?2"
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query(params![limit, offset])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(map_channel(row)?);
    }
    Ok(out)
}

/// Moves one favorite immediately before another in the global favorite lineup.
/// Returns `false` when either key is absent.
///
/// # Errors
/// Returns [`DbError`](crate::error::DbError) on a query or write failure.
pub fn move_before(
    conn: &mut Connection,
    target: (SourceId, ChannelIdentity),
    anchor: (SourceId, ChannelIdentity),
) -> DbResult<bool> {
    reorder(conn, target, anchor, false)
}

/// Moves one favorite immediately after another in the global favorite lineup.
/// Returns `false` when either key is absent.
///
/// # Errors
/// Returns [`DbError`](crate::error::DbError) on a query or write failure.
pub fn move_after(
    conn: &mut Connection,
    target: (SourceId, ChannelIdentity),
    anchor: (SourceId, ChannelIdentity),
) -> DbResult<bool> {
    reorder(conn, target, anchor, true)
}

fn reorder(
    conn: &mut Connection,
    target: (SourceId, ChannelIdentity),
    anchor: (SourceId, ChannelIdentity),
    after: bool,
) -> DbResult<bool> {
    if target == anchor {
        return is_favorite(conn, target.0, target.1);
    }
    let transaction = conn.transaction()?;
    let mut ordered = {
        let mut statement = transaction.prepare(
            "SELECT source_id, identity FROM favorites ORDER BY position, created_at_unix DESC",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                SourceId::new(row.get(0)?),
                ChannelIdentity::from_storage(row.get(1)?),
            ))
        })?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    let Some(target_index) = ordered.iter().position(|key| *key == target) else {
        return Ok(false);
    };
    let moved = ordered.remove(target_index);
    let Some(anchor_index) = ordered.iter().position(|key| *key == anchor) else {
        return Ok(false);
    };
    let insert_at = anchor_index + usize::from(after);
    ordered.insert(insert_at, moved);

    let mut update = transaction
        .prepare("UPDATE favorites SET position = ?1 WHERE source_id = ?2 AND identity = ?3")?;
    for (position, (source, identity)) in ordered.into_iter().enumerate() {
        let position = i64::try_from(position).unwrap_or(i64::MAX);
        update.execute(params![position, source.value(), identity.to_storage()])?;
    }
    drop(update);
    transaction.commit()?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::pool::Db;
    use crate::repo::channels::{IMPORT_COLUMNS, NewChannel, insert_into};
    use core_model::channel::{ChannelOverrides, MediaKind, channel_identity};
    use core_model::locator::StreamLocator;

    fn seed_source(conn: &Connection) -> SourceId {
        conn.execute(
            "INSERT INTO sources(id, kind, name) VALUES (1, 'm3u-url', 'S')",
            [],
        )
        .unwrap();
        SourceId::new(1)
    }

    fn insert_channel(conn: &Connection, source: SourceId, name: &str) -> ChannelIdentity {
        let url = format!("http://host/live/{name}");
        let ch = NewChannel {
            identity: channel_identity(None, &url, name),
            epg_key: None,
            name: name.to_owned(),
            group_title: None,
            logo: None,
            locator: StreamLocator::parse(&url).unwrap(),
            kind: MediaKind::Live,
            category: None,
            overrides: ChannelOverrides::default(),
        };
        let sql = format!(
            "INSERT INTO channels({IMPORT_COLUMNS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)"
        );
        let mut stmt = conn.prepare(&sql).unwrap();
        insert_into(&mut stmt, source, &ch, 0).unwrap();
        ch.identity
    }

    #[test]
    fn favorite_channels_resolve_and_respect_enabled() {
        let db = Db::open_in_memory().unwrap();
        let conn = db.writer();
        let src = seed_source(&conn);
        let ident = insert_channel(&conn, src, "BBC");
        add(&conn, src, ident, 100).unwrap();

        // A favorite with no matching channel row is skipped.
        add(&conn, src, ChannelIdentity::from_raw(4242), 200).unwrap();

        let resolved = list_channels(&conn, 0, 10).unwrap();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].name, "BBC");
        assert_eq!(count_channels(&conn).unwrap(), 1);

        // Disabling the source hides its favorites from the home row.
        conn.execute("UPDATE sources SET enabled = 0 WHERE id = 1", [])
            .unwrap();
        assert!(list_channels(&conn, 0, 10).unwrap().is_empty());
        assert_eq!(count_channels(&conn).unwrap(), 0);
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

    #[test]
    fn moving_a_missing_favorite_relative_to_itself_reports_absent() {
        let db = Db::open_in_memory().unwrap();
        let mut conn = db.writer();
        let source = seed_source(&conn);
        let identity = ChannelIdentity::from_raw(99);

        assert!(!move_before(&mut conn, (source, identity), (source, identity)).unwrap());

        add(&conn, source, identity, 100).unwrap();
        assert!(move_after(&mut conn, (source, identity), (source, identity)).unwrap());
    }
}
