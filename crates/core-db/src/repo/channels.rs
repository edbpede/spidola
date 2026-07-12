// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Channels repository: read paths for browse, plus the row shape the staging-and-swap
//! refresh ([`crate::refresh`]) writes. Hidden flags key on the stable identity, not the
//! rowid, so they survive a refresh (TECH_SPEC §4.4).

use rusqlite::{Connection, Row, params};

use core_model::channel::{Channel, ChannelOverrides, MediaKind};
use core_model::ids::{CategoryId, ChannelId, ChannelIdentity, SourceId};
use core_model::locator::StreamLocator;

use crate::error::{DbError, DbResult};

/// A channel to import. Built by the importer from a parsed playlist entry, with its
/// [`ChannelIdentity`] already derived and its locator already validated.
#[derive(Debug, Clone)]
pub struct NewChannel {
    /// Stable per-source identity.
    pub identity: ChannelIdentity,
    /// Display name.
    pub name: String,
    /// Group / category label, if any.
    pub group_title: Option<String>,
    /// Logo URL, if any.
    pub logo: Option<String>,
    /// Validated stream locator.
    pub locator: StreamLocator,
    /// What the channel plays.
    pub kind: MediaKind,
    /// Resolved category, if any.
    pub category: Option<CategoryId>,
    /// Per-channel overrides.
    pub overrides: ChannelOverrides,
}

/// Columns shared by the live `channels` table and the refresh staging table, in order.
pub(crate) const IMPORT_COLUMNS: &str = "source_id, identity, name, group_title, logo, locator, kind, \
     category_id, user_agent, headers, preferred_engine, sort_index";

/// The stored string for a [`MediaKind`] (matches the `is_live` generated-column check).
pub(crate) const fn kind_to_str(kind: MediaKind) -> &'static str {
    match kind {
        MediaKind::Live => "live",
        MediaKind::Movie => "movie",
        MediaKind::SeriesEpisode => "series-episode",
    }
}

fn kind_from_str(raw: &str) -> DbResult<MediaKind> {
    match raw {
        "live" => Ok(MediaKind::Live),
        "movie" => Ok(MediaKind::Movie),
        "series-episode" => Ok(MediaKind::SeriesEpisode),
        other => Err(DbError::Integrity(format!("unknown media kind `{other}`"))),
    }
}

fn headers_to_json(overrides: &ChannelOverrides) -> DbResult<Option<String>> {
    if overrides.headers.is_empty() {
        return Ok(None);
    }
    Ok(Some(serde_json::to_string(&overrides.headers)?))
}

fn headers_from_json(raw: Option<String>) -> DbResult<Vec<(String, String)>> {
    match raw {
        Some(json) => Ok(serde_json::from_str(&json)?),
        None => Ok(Vec::new()),
    }
}

/// Binds a [`NewChannel`] to the shared import column order. Used by the staging insert.
pub(crate) fn insert_into(
    stmt: &mut rusqlite::Statement<'_>,
    source: SourceId,
    channel: &NewChannel,
    sort_index: i64,
) -> DbResult<()> {
    stmt.execute(params![
        source.value(),
        channel.identity.to_storage(),
        channel.name,
        channel.group_title,
        channel.logo,
        channel.locator.as_str(),
        kind_to_str(channel.kind),
        channel.category.map(CategoryId::value),
        channel.overrides.user_agent,
        headers_to_json(&channel.overrides)?,
        channel.overrides.preferred_engine,
        sort_index,
    ])?;
    Ok(())
}

fn map_channel(row: &Row<'_>) -> DbResult<Channel> {
    let locator_raw: String = row.get("locator")?;
    let locator = StreamLocator::parse(&locator_raw)
        .map_err(|e| DbError::Integrity(format!("stored locator is invalid: {e}")))?;
    let kind = kind_from_str(&row.get::<_, String>("kind")?)?;
    let overrides = ChannelOverrides {
        user_agent: row.get("user_agent")?,
        headers: headers_from_json(row.get("headers")?)?,
        preferred_engine: row.get("preferred_engine")?,
    };
    Ok(Channel {
        id: ChannelId::new(row.get("id")?),
        source_id: SourceId::new(row.get("source_id")?),
        identity: ChannelIdentity::from_storage(row.get("identity")?),
        name: row.get("name")?,
        group_title: row.get("group_title")?,
        logo: row.get("logo")?,
        locator,
        kind,
        category: row
            .get::<_, Option<i64>>("category_id")?
            .map(CategoryId::new),
        overrides,
    })
}

const SELECT_COLUMNS: &str = "id, source_id, identity, name, group_title, logo, locator, \
                              kind, category_id, user_agent, headers, preferred_engine";

/// Counts the channels currently in a source's catalog.
///
/// # Errors
/// Returns [`DbError`] on a query failure.
pub fn count_for_source(conn: &Connection, source: SourceId) -> DbResult<u64> {
    let n: i64 = conn.query_row(
        "SELECT count(*) FROM channels WHERE source_id = ?1",
        params![source.value()],
        |r| r.get(0),
    )?;
    Ok(u64::try_from(n).unwrap_or(0))
}

/// Lists a page of a source's channels in playlist order (paged by contract, §4.6).
///
/// # Errors
/// Returns [`DbError`] on a query or row-mapping failure.
pub fn list_for_source(
    conn: &Connection,
    source: SourceId,
    offset: u32,
    limit: u32,
) -> DbResult<Vec<Channel>> {
    let sql = format!(
        "SELECT {SELECT_COLUMNS} FROM channels WHERE source_id = ?1 \
         ORDER BY sort_index LIMIT ?2 OFFSET ?3"
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query(params![source.value(), limit, offset])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(map_channel(row)?);
    }
    Ok(out)
}

/// Fetches one channel by rowid.
///
/// # Errors
/// Returns [`DbError`] on a query or row-mapping failure.
pub fn get(conn: &Connection, id: ChannelId) -> DbResult<Option<Channel>> {
    let sql = format!("SELECT {SELECT_COLUMNS} FROM channels WHERE id = ?1");
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query(params![id.value()])?;
    match rows.next()? {
        Some(row) => Ok(Some(map_channel(row)?)),
        None => Ok(None),
    }
}

/// Marks a channel hidden by stable identity (survives refresh).
///
/// # Errors
/// Returns [`DbError`] on a write failure.
pub fn hide(conn: &Connection, source: SourceId, identity: ChannelIdentity) -> DbResult<()> {
    conn.execute(
        "INSERT OR IGNORE INTO hidden_channels(source_id, identity) VALUES (?1, ?2)",
        params![source.value(), identity.to_storage()],
    )?;
    Ok(())
}

/// Reverses [`hide`].
///
/// # Errors
/// Returns [`DbError`] on a write failure.
pub fn unhide(conn: &Connection, source: SourceId, identity: ChannelIdentity) -> DbResult<()> {
    conn.execute(
        "DELETE FROM hidden_channels WHERE source_id = ?1 AND identity = ?2",
        params![source.value(), identity.to_storage()],
    )?;
    Ok(())
}

/// Whether a channel is hidden.
///
/// # Errors
/// Returns [`DbError`] on a query failure.
pub fn is_hidden(conn: &Connection, source: SourceId, identity: ChannelIdentity) -> DbResult<bool> {
    let n: i64 = conn.query_row(
        "SELECT count(*) FROM hidden_channels WHERE source_id = ?1 AND identity = ?2",
        params![source.value(), identity.to_storage()],
        |r| r.get(0),
    )?;
    Ok(n > 0)
}
