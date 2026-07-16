// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Persistence for user-created channel groups (PRD §6.7).

use rusqlite::{Connection, OptionalExtension, Row, params};

use core_model::{CustomChannel, CustomChannelId, CustomGroup, CustomGroupId, StreamLocator};

use crate::error::DbResult;

/// Creates a group at the end of the user lineup.
///
/// # Errors
/// Returns a database error if the group cannot be inserted.
pub fn create_group(conn: &Connection, name: &str) -> DbResult<CustomGroupId> {
    conn.execute(
        "INSERT INTO custom_groups(name, position) \
         SELECT ?1, coalesce(max(position) + 1, 0) FROM custom_groups",
        params![name],
    )?;
    Ok(CustomGroupId::new(conn.last_insert_rowid()))
}

/// Lists a page of custom groups.
///
/// # Errors
/// Returns a database error if the page cannot be queried.
pub fn list_groups(conn: &Connection, offset: u32, limit: u32) -> DbResult<Vec<CustomGroup>> {
    let mut statement = conn.prepare(
        "SELECT id, name, position FROM custom_groups ORDER BY position, id LIMIT ?1 OFFSET ?2",
    )?;
    let rows = statement.query_map(params![limit, offset], |row| {
        Ok(CustomGroup {
            id: CustomGroupId::new(row.get("id")?),
            name: row.get("name")?,
            position: row.get("position")?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

/// Whether a custom group exists.
///
/// # Errors
/// Returns a database error if the lookup fails.
pub fn group_exists(conn: &Connection, id: CustomGroupId) -> DbResult<bool> {
    let count: i64 = conn.query_row(
        "SELECT count(*) FROM custom_groups WHERE id = ?1",
        params![id.value()],
        |row| row.get(0),
    )?;
    Ok(count == 1)
}

/// Renames a custom group. Returns whether it existed.
///
/// # Errors
/// Returns a database error if the update fails.
pub fn rename_group(conn: &Connection, id: CustomGroupId, name: &str) -> DbResult<bool> {
    Ok(conn.execute(
        "UPDATE custom_groups SET name = ?2 WHERE id = ?1",
        params![id.value(), name],
    )? == 1)
}

/// Deletes a group. Its channels remain and move to the ungrouped lineup by foreign-key rule.
///
/// # Errors
/// Returns a database error if the delete fails.
pub fn delete_group(conn: &Connection, id: CustomGroupId) -> DbResult<bool> {
    Ok(conn.execute(
        "DELETE FROM custom_groups WHERE id = ?1",
        params![id.value()],
    )? == 1)
}

/// Moves one group immediately before another.
///
/// # Errors
/// Returns a database error if the lineup cannot be read or rewritten.
pub fn move_group_before(
    conn: &mut Connection,
    target: CustomGroupId,
    anchor: CustomGroupId,
) -> DbResult<bool> {
    reorder_groups(conn, target, anchor, false)
}

/// Moves one group immediately after another.
///
/// # Errors
/// Returns a database error if the lineup cannot be read or rewritten.
pub fn move_group_after(
    conn: &mut Connection,
    target: CustomGroupId,
    anchor: CustomGroupId,
) -> DbResult<bool> {
    reorder_groups(conn, target, anchor, true)
}

fn reorder_groups(
    conn: &mut Connection,
    target: CustomGroupId,
    anchor: CustomGroupId,
    after: bool,
) -> DbResult<bool> {
    if target == anchor {
        return group_exists(conn, target);
    }
    let transaction = conn.transaction()?;
    let mut ordered = {
        let mut statement =
            transaction.prepare("SELECT id FROM custom_groups ORDER BY position, id")?;
        let rows = statement.query_map([], |row| row.get::<_, i64>(0).map(CustomGroupId::new))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    let Some(target_index) = ordered.iter().position(|id| *id == target) else {
        return Ok(false);
    };
    let moved = ordered.remove(target_index);
    let Some(anchor_index) = ordered.iter().position(|id| *id == anchor) else {
        return Ok(false);
    };
    ordered.insert(anchor_index + usize::from(after), moved);
    let mut update = transaction.prepare("UPDATE custom_groups SET position = ?1 WHERE id = ?2")?;
    for (position, id) in ordered.into_iter().enumerate() {
        update.execute(params![
            i64::try_from(position).unwrap_or(i64::MAX),
            id.value()
        ])?;
    }
    drop(update);
    transaction.commit()?;
    Ok(true)
}

/// Finds a group by its user-facing name.
///
/// # Errors
/// Returns a database error if the lookup fails.
pub fn group_id_by_name(conn: &Connection, name: &str) -> DbResult<Option<CustomGroupId>> {
    conn.query_row(
        "SELECT id FROM custom_groups WHERE name = ?1",
        params![name],
        |row| row.get::<_, i64>(0).map(CustomGroupId::new),
    )
    .optional()
    .map_err(Into::into)
}

/// Creates a sealed custom channel at the end of its group.
///
/// # Errors
/// Returns a database error if serialization or insertion fails.
pub fn create_channel(conn: &Connection, channel: &CustomChannel) -> DbResult<CustomChannelId> {
    let headers = serde_json::to_string(&channel.headers)?;
    conn.execute(
        "INSERT INTO custom_channels(group_id, name, logo, locator, user_agent, headers, position) \
         SELECT ?1, ?2, ?3, ?4, ?5, ?6, coalesce(max(position) + 1, 0) \
         FROM custom_channels WHERE group_id IS ?1",
        params![
            channel.group_id.map(CustomGroupId::value),
            channel.name,
            channel.logo,
            channel.locator.as_str(),
            channel.user_agent,
            headers,
        ],
    )?;
    Ok(CustomChannelId::new(conn.last_insert_rowid()))
}

/// Updates a sealed custom channel. Returns whether the row existed.
///
/// # Errors
/// Returns a database error if serialization or the update fails.
pub fn update_channel(conn: &Connection, channel: &CustomChannel) -> DbResult<bool> {
    let headers = serde_json::to_string(&channel.headers)?;
    let changed = conn.execute(
        "UPDATE custom_channels SET \
         position = CASE WHEN group_id IS ?1 THEN position ELSE \
             coalesce((SELECT max(other.position) + 1 FROM custom_channels other \
                       WHERE other.group_id IS ?1), 0) END, \
         group_id = ?1, name = ?2, logo = ?3, locator = ?4, \
         user_agent = ?5, headers = ?6 WHERE id = ?7",
        params![
            channel.group_id.map(CustomGroupId::value),
            channel.name,
            channel.logo,
            channel.locator.as_str(),
            channel.user_agent,
            headers,
            channel.id.value(),
        ],
    )?;
    Ok(changed == 1)
}

/// Deletes a custom channel idempotently.
///
/// # Errors
/// Returns a database error if the delete fails.
pub fn delete_channel(conn: &Connection, id: CustomChannelId) -> DbResult<()> {
    conn.execute(
        "DELETE FROM custom_channels WHERE id = ?1",
        params![id.value()],
    )?;
    Ok(())
}

/// Moves one custom channel immediately before another, including across groups.
///
/// # Errors
/// Returns a database error if the lineup cannot be read or rewritten.
pub fn move_channel_before(
    conn: &mut Connection,
    target: CustomChannelId,
    anchor: CustomChannelId,
) -> DbResult<bool> {
    reorder_channels(conn, target, anchor, false)
}

/// Moves one custom channel immediately after another, including across groups.
///
/// # Errors
/// Returns a database error if the lineup cannot be read or rewritten.
pub fn move_channel_after(
    conn: &mut Connection,
    target: CustomChannelId,
    anchor: CustomChannelId,
) -> DbResult<bool> {
    reorder_channels(conn, target, anchor, true)
}

fn reorder_channels(
    conn: &mut Connection,
    target: CustomChannelId,
    anchor: CustomChannelId,
    after: bool,
) -> DbResult<bool> {
    if target == anchor {
        return Ok(get_channel(conn, target)?.is_some());
    }
    let transaction = conn.transaction()?;
    let anchor_group = transaction
        .query_row(
            "SELECT group_id FROM custom_channels WHERE id = ?1",
            params![anchor.value()],
            |row| row.get::<_, Option<i64>>(0),
        )
        .optional()?;
    let Some(anchor_group) = anchor_group else {
        return Ok(false);
    };
    let target_exists: bool = transaction.query_row(
        "SELECT count(*) = 1 FROM custom_channels WHERE id = ?1",
        params![target.value()],
        |row| row.get(0),
    )?;
    if !target_exists {
        return Ok(false);
    }
    transaction.execute(
        "UPDATE custom_channels SET group_id = ?2 WHERE id = ?1",
        params![target.value(), anchor_group],
    )?;
    let mut ordered = {
        let mut statement = transaction
            .prepare("SELECT id FROM custom_channels WHERE group_id IS ?1 ORDER BY position, id")?;
        let rows = statement.query_map(params![anchor_group], |row| {
            row.get::<_, i64>(0).map(CustomChannelId::new)
        })?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    let Some(target_index) = ordered.iter().position(|id| *id == target) else {
        return Ok(false);
    };
    let moved = ordered.remove(target_index);
    let Some(anchor_index) = ordered.iter().position(|id| *id == anchor) else {
        return Ok(false);
    };
    ordered.insert(anchor_index + usize::from(after), moved);
    let mut update =
        transaction.prepare("UPDATE custom_channels SET position = ?1 WHERE id = ?2")?;
    for (position, id) in ordered.into_iter().enumerate() {
        update.execute(params![
            i64::try_from(position).unwrap_or(i64::MAX),
            id.value()
        ])?;
    }
    drop(update);
    transaction.commit()?;
    Ok(true)
}

/// Fetches one custom channel.
///
/// # Errors
/// Returns a database error if the row cannot be read or decoded.
pub fn get_channel(conn: &Connection, id: CustomChannelId) -> DbResult<Option<CustomChannel>> {
    conn.query_row(
        "SELECT id, group_id, name, logo, locator, user_agent, headers, position \
         FROM custom_channels WHERE id = ?1",
        params![id.value()],
        map_channel,
    )
    .optional()
    .map_err(Into::into)
}

/// Lists a bounded page of custom channels, optionally within one group.
///
/// # Errors
/// Returns a database error if the page cannot be read or decoded.
pub fn list_channels(
    conn: &Connection,
    group: Option<CustomGroupId>,
    offset: u32,
    limit: u32,
) -> DbResult<Vec<CustomChannel>> {
    let mut statement = conn.prepare(
        "SELECT id, group_id, name, logo, locator, user_agent, headers, position \
         FROM custom_channels WHERE group_id IS ?1 ORDER BY position, id LIMIT ?2 OFFSET ?3",
    )?;
    let rows = statement.query_map(
        params![group.map(CustomGroupId::value), limit, offset],
        map_channel,
    )?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

/// Lists every custom channel in deterministic group/position order for portable export.
///
/// # Errors
/// Returns a database error if the catalog cannot be read or decoded.
pub fn list_all_channels(conn: &Connection) -> DbResult<Vec<CustomChannel>> {
    let mut statement = conn.prepare(
        "SELECT id, group_id, name, logo, locator, user_agent, headers, position \
         FROM custom_channels ORDER BY coalesce(group_id, 0), position, id",
    )?;
    let rows = statement.query_map([], map_channel)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

/// Clears the custom catalog inside a caller-owned transaction.
///
/// # Errors
/// Returns a database error if either delete fails.
pub fn clear(conn: &Connection) -> DbResult<()> {
    conn.execute("DELETE FROM custom_channels", [])?;
    conn.execute("DELETE FROM custom_groups", [])?;
    Ok(())
}

fn map_channel(row: &Row<'_>) -> rusqlite::Result<CustomChannel> {
    let locator_raw: String = row.get("locator")?;
    let locator = StreamLocator::parse(&locator_raw).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            locator_raw.len(),
            rusqlite::types::Type::Text,
            Box::new(error),
        )
    })?;
    let headers: String = row.get("headers")?;
    let headers = serde_json::from_str(&headers).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            headers.len(),
            rusqlite::types::Type::Text,
            Box::new(error),
        )
    })?;
    Ok(CustomChannel {
        id: CustomChannelId::new(row.get("id")?),
        group_id: row
            .get::<_, Option<i64>>("group_id")?
            .map(CustomGroupId::new),
        name: row.get("name")?,
        logo: row.get("logo")?,
        locator,
        user_agent: row.get("user_agent")?,
        headers,
        position: row.get("position")?,
    })
}
