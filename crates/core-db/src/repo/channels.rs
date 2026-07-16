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
    /// Source-scoped XMLTV/Xtream EPG key, when the catalog provided one.
    pub epg_key: Option<String>,
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
pub(crate) const IMPORT_COLUMNS: &str = "source_id, identity, epg_key, name, group_title, logo, locator, kind, \
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
        channel.epg_key,
        channel.name,
        channel.group_title,
        channel.logo,
        channel.locator.as_str(),
        kind_to_str(channel.kind),
        channel.category.map(CategoryId::value),
        channel.overrides.user_agent,
        headers_to_json(&channel.overrides)?,
        channel.overrides.preferred_engine,
        sort_index
    ])?;
    Ok(())
}

pub(crate) fn map_channel(row: &Row<'_>) -> DbResult<Channel> {
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

/// The [`SELECT_COLUMNS`] list qualified with a table alias, for queries that join `channels`
/// against another table sharing column names (e.g. resolving favorites). The result column
/// names stay unqualified, so [`map_channel`] reads them by their bare names unchanged.
pub(crate) fn prefixed_select_columns(alias: &str) -> String {
    SELECT_COLUMNS
        .split(',')
        .map(|column| format!("{alias}.{}", column.trim()))
        .collect::<Vec<_>>()
        .join(", ")
}

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

/// Resolves one current catalog row by its refresh-stable identity.
///
/// # Errors
/// Returns [`DbError`] on a query failure or corrupt stored row.
pub fn get_by_identity(
    conn: &Connection,
    source: SourceId,
    identity: ChannelIdentity,
) -> DbResult<Option<Channel>> {
    let sql =
        format!("SELECT {SELECT_COLUMNS} FROM channels WHERE source_id = ?1 AND identity = ?2");
    let mut statement = conn.prepare(&sql)?;
    let mut rows = statement.query(params![source.value(), identity.to_storage()])?;
    rows.next()?.map(map_channel).transpose()
}

/// Resolves every source-scoped EPG key to the stable channel identity used by favorites.
///
/// # Errors
/// Returns [`DbError`] on a query failure.
pub fn epg_identity_map(
    conn: &Connection,
    source: SourceId,
) -> DbResult<std::collections::HashMap<String, ChannelIdentity>> {
    let mut statement = conn.prepare(
        "SELECT epg_key, identity FROM channels \
         WHERE source_id = ?1 AND epg_key IS NOT NULL",
    )?;
    let rows = statement.query_map(params![source.value()], |row| {
        Ok((
            row.get::<_, String>(0)?,
            ChannelIdentity::from_storage(row.get::<_, i64>(1)?),
        ))
    })?;
    rows.collect::<Result<_, _>>().map_err(Into::into)
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

/// The correlated sub-query that excludes hidden channels from a browse query. Hidden flags
/// key on the stable `(source_id, identity)`, so they survive a refresh (§4.4); browse must
/// filter them out without a join that would perturb paging counts.
const NOT_HIDDEN: &str = "NOT EXISTS (SELECT 1 FROM hidden_channels h \
     WHERE h.source_id = channels.source_id AND h.identity = channels.identity)";

/// A distinct `group_title` within a source's catalog (a "category" in the browse drill-down),
/// with the number of visible channels it holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupSummary {
    /// The playlist group label; `None` is the "ungrouped" bucket.
    pub title: Option<String>,
    /// Visible (non-hidden) channels in this group.
    pub channel_count: u64,
}

/// Lists the distinct media kinds present in a source's catalog, in display order
/// (live, movie, series). The "type" level of the browse drill-down (source → type →
/// category → channel); for an M3U source this is just `[Live]`.
///
/// # Errors
/// Returns [`DbError`] on a query or mapping failure.
pub fn kinds_for_source(conn: &Connection, source: SourceId) -> DbResult<Vec<MediaKind>> {
    let mut stmt = conn.prepare("SELECT DISTINCT kind FROM channels WHERE source_id = ?1")?;
    let mut rows = stmt.query(params![source.value()])?;
    let mut kinds = Vec::new();
    while let Some(row) = rows.next()? {
        kinds.push(kind_from_str(&row.get::<_, String>("kind")?)?);
    }
    // Stable display order regardless of insertion order.
    kinds.sort_unstable_by_key(|k| match k {
        MediaKind::Live => 0,
        MediaKind::Movie => 1,
        MediaKind::SeriesEpisode => 2,
    });
    Ok(kinds)
}

/// Counts the distinct visible groups for a source and media kind (paging total for
/// [`browse_groups`]).
///
/// # Errors
/// Returns [`DbError`] on a query failure.
pub fn count_groups(conn: &Connection, source: SourceId, kind: MediaKind) -> DbResult<u64> {
    let sql = format!(
        "SELECT count(*) FROM (SELECT 1 FROM channels \
         WHERE source_id = ?1 AND kind = ?2 AND {NOT_HIDDEN} GROUP BY group_title)"
    );
    let n: i64 = conn.query_row(&sql, params![source.value(), kind_to_str(kind)], |r| {
        r.get(0)
    })?;
    Ok(u64::try_from(n).unwrap_or(0))
}

/// Lists a page of a source's distinct groups for a media kind, ungrouped last, otherwise
/// case-insensitive by title (paged by contract, §4.6).
///
/// # Errors
/// Returns [`DbError`] on a query or mapping failure.
pub fn browse_groups(
    conn: &Connection,
    source: SourceId,
    kind: MediaKind,
    offset: u32,
    limit: u32,
) -> DbResult<Vec<GroupSummary>> {
    let sql = format!(
        "SELECT group_title, count(*) AS n FROM channels \
         WHERE source_id = ?1 AND kind = ?2 AND {NOT_HIDDEN} \
         GROUP BY group_title \
         ORDER BY (group_title IS NULL), group_title COLLATE NOCASE \
         LIMIT ?3 OFFSET ?4"
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query(params![source.value(), kind_to_str(kind), limit, offset])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(GroupSummary {
            title: row.get("group_title")?,
            channel_count: u64::try_from(row.get::<_, i64>("n")?).unwrap_or(0),
        });
    }
    Ok(out)
}

/// Counts the visible channels in one group of a source and media kind (paging total for
/// [`list_in_group`]). `group` is the group title; `None` selects the ungrouped bucket.
///
/// # Errors
/// Returns [`DbError`] on a query failure.
pub fn count_in_group(
    conn: &Connection,
    source: SourceId,
    kind: MediaKind,
    group: Option<&str>,
) -> DbResult<u64> {
    let sql = format!(
        "SELECT count(*) FROM channels \
         WHERE source_id = ?1 AND kind = ?2 \
         AND ((?3 IS NULL AND group_title IS NULL) OR group_title = ?3) AND {NOT_HIDDEN}"
    );
    let n: i64 = conn.query_row(
        &sql,
        params![source.value(), kind_to_str(kind), group],
        |r| r.get(0),
    )?;
    Ok(u64::try_from(n).unwrap_or(0))
}

/// Lists a page of the visible channels in one group of a source and media kind, in playlist
/// order (paged by contract). `group` is the group title; `None` selects the ungrouped bucket.
///
/// # Errors
/// Returns [`DbError`] on a query or row-mapping failure.
pub fn list_in_group(
    conn: &Connection,
    source: SourceId,
    kind: MediaKind,
    group: Option<&str>,
    offset: u32,
    limit: u32,
) -> DbResult<Vec<Channel>> {
    let sql = format!(
        "SELECT {SELECT_COLUMNS} FROM channels \
         WHERE source_id = ?1 AND kind = ?2 \
         AND ((?3 IS NULL AND group_title IS NULL) OR group_title = ?3) AND {NOT_HIDDEN} \
         ORDER BY sort_index LIMIT ?4 OFFSET ?5"
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query(params![
        source.value(),
        kind_to_str(kind),
        group,
        limit,
        offset
    ])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(map_channel(row)?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::pool::Db;
    use core_model::channel::channel_identity;

    fn seed_source(conn: &Connection) -> SourceId {
        conn.execute(
            "INSERT INTO sources(id, kind, name) VALUES (1, 'm3u-url', 'S')",
            [],
        )
        .unwrap();
        SourceId::new(1)
    }

    fn channel(name: &str, group: Option<&str>) -> NewChannel {
        let url = format!("http://host/live/{name}");
        NewChannel {
            identity: channel_identity(None, &url, name),
            epg_key: None,
            name: name.to_owned(),
            group_title: group.map(str::to_owned),
            logo: None,
            locator: StreamLocator::parse(&url).unwrap(),
            kind: MediaKind::Live,
            category: None,
            overrides: ChannelOverrides::default(),
        }
    }

    fn insert(conn: &Connection, source: SourceId, channels: &[NewChannel]) {
        let sql = format!(
            "INSERT INTO channels({IMPORT_COLUMNS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)"
        );
        let mut stmt = conn.prepare(&sql).unwrap();
        for (i, ch) in channels.iter().enumerate() {
            insert_into(&mut stmt, source, ch, i64::try_from(i).unwrap()).unwrap();
        }
    }

    #[test]
    fn groups_are_distinct_ungrouped_last_and_counted() {
        let db = Db::open_in_memory().unwrap();
        let conn = db.writer();
        let src = seed_source(&conn);
        insert(
            &conn,
            src,
            &[
                channel("A", Some("News")),
                channel("B", Some("News")),
                channel("C", Some("Sports")),
                channel("D", None),
            ],
        );
        let groups = browse_groups(&conn, src, MediaKind::Live, 0, 100).unwrap();
        assert_eq!(count_groups(&conn, src, MediaKind::Live).unwrap(), 3);
        assert_eq!(
            groups,
            vec![
                GroupSummary {
                    title: Some("News".to_owned()),
                    channel_count: 2,
                },
                GroupSummary {
                    title: Some("Sports".to_owned()),
                    channel_count: 1,
                },
                GroupSummary {
                    title: None,
                    channel_count: 1,
                },
            ]
        );
    }

    #[test]
    fn hidden_channels_drop_out_of_browse() {
        let db = Db::open_in_memory().unwrap();
        let conn = db.writer();
        let src = seed_source(&conn);
        let news = [channel("A", Some("News")), channel("B", Some("News"))];
        insert(&conn, src, &news);
        hide(&conn, src, news[0].identity).unwrap();

        assert_eq!(
            count_in_group(&conn, src, MediaKind::Live, Some("News")).unwrap(),
            1
        );
        let visible = list_in_group(&conn, src, MediaKind::Live, Some("News"), 0, 10).unwrap();
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].name, "B");
        // The group still exists but with a reduced count.
        let groups = browse_groups(&conn, src, MediaKind::Live, 0, 10).unwrap();
        assert_eq!(groups[0].channel_count, 1);
    }

    #[test]
    fn ungrouped_bucket_selects_null_group() {
        let db = Db::open_in_memory().unwrap();
        let conn = db.writer();
        let src = seed_source(&conn);
        insert(
            &conn,
            src,
            &[channel("A", None), channel("B", Some("News"))],
        );
        let ungrouped = list_in_group(&conn, src, MediaKind::Live, None, 0, 10).unwrap();
        assert_eq!(ungrouped.len(), 1);
        assert_eq!(ungrouped[0].name, "A");
    }

    #[test]
    fn kinds_are_deduplicated_in_display_order() {
        let db = Db::open_in_memory().unwrap();
        let conn = db.writer();
        let src = seed_source(&conn);
        insert(&conn, src, &[channel("A", None), channel("B", Some("X"))]);
        assert_eq!(kinds_for_source(&conn, src).unwrap(), vec![MediaKind::Live]);
    }
}
