// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! `core-search` — the query layer over FTS5 (sub-50 ms at 50k channels, PRD §9).
//!
//! The hot path compiles user text to an FTS5 prefix `MATCH` ([`query`]) and joins the
//! contentless index back to `channels`, ordered by BM25 `rank`. When the prefix finds
//! nothing, a bounded trigram fuzzy fallback ([`ranking`]) recovers typos. Every method is
//! paged by contract (offset/limit), so no unbounded result set is ever produced. This
//! crate reads a `rusqlite::Connection` directly — it is the read query layer — and knows
//! only the shared schema contract, not `core-db`'s internals.
#![forbid(unsafe_code)]

pub mod error;
pub mod query;
pub mod ranking;

use rusqlite::types::Value;
use rusqlite::{Connection, Row, params_from_iter};

use core_model::channel::{Channel, ChannelOverrides, MediaKind};
use core_model::ids::{CategoryId, ChannelId, ChannelIdentity, SourceId};
use core_model::locator::StreamLocator;

pub use error::{SearchError, SearchResult};

/// Upper bound on candidate rows the fuzzy fallback scores, keeping its worst case bounded
/// even though it only fires when the prefix path returns nothing. Candidates are drawn
/// round-robin across sources (see [`fuzzy_candidates`]) so no single large source can
/// consume the whole budget and hide a later source's channels from typo search.
const MAX_FUZZY_SCAN: usize = 20_000;

/// Minimum trigram similarity for a fuzzy candidate to be returned.
const FUZZY_THRESHOLD: f32 = 0.30;

/// A search request. Paged by contract via `offset`/`limit`.
#[derive(Debug, Clone)]
pub struct SearchRequest<'a> {
    /// Raw user query text.
    pub text: &'a str,
    /// Restrict to one source, if set.
    pub source: Option<SourceId>,
    /// Restrict to one media kind, if set.
    pub kind: Option<MediaKind>,
    /// Page offset.
    pub offset: u32,
    /// Page size.
    pub limit: u32,
}

/// A page of search results plus whether the fuzzy fallback produced them.
#[derive(Debug, Clone)]
pub struct SearchPage {
    /// Matching channels, most relevant first.
    pub channels: Vec<Channel>,
    /// `true` when these came from the trigram fallback rather than the prefix index.
    pub fuzzy: bool,
}

const SELECT_COLUMNS: &str = "c.id, c.source_id, c.identity, c.name, c.group_title, \
                              c.logo, c.locator, c.kind, c.category_id";

const fn kind_to_str(kind: MediaKind) -> &'static str {
    match kind {
        MediaKind::Live => "live",
        MediaKind::Movie => "movie",
        MediaKind::SeriesEpisode => "series-episode",
    }
}

fn kind_from_str(raw: &str) -> SearchResult<MediaKind> {
    match raw {
        "live" => Ok(MediaKind::Live),
        "movie" => Ok(MediaKind::Movie),
        "series-episode" => Ok(MediaKind::SeriesEpisode),
        other => Err(SearchError::Integrity(format!(
            "unknown media kind `{other}`"
        ))),
    }
}

/// Maps a search-result row to a display-complete [`Channel`]. Per-channel overrides are
/// intentionally not selected on the hot path (fewer columns = faster); the service layer
/// re-fetches the full channel by id when the user actually plays one.
fn map_row(row: &Row<'_>) -> SearchResult<Channel> {
    let locator_raw: String = row.get("locator")?;
    let locator = StreamLocator::parse(&locator_raw)
        .map_err(|e| SearchError::Integrity(format!("stored locator is invalid: {e}")))?;
    Ok(Channel {
        id: ChannelId::new(row.get("id")?),
        source_id: SourceId::new(row.get("source_id")?),
        identity: ChannelIdentity::from_storage(row.get("identity")?),
        name: row.get("name")?,
        group_title: row.get("group_title")?,
        logo: row.get("logo")?,
        locator,
        kind: kind_from_str(&row.get::<_, String>("kind")?)?,
        category: row
            .get::<_, Option<i64>>("category_id")?
            .map(CategoryId::new),
        overrides: ChannelOverrides::default(),
    })
}

/// Appends the shared `source`/`kind`/hidden filters and returns their bound values.
fn push_filters(sql: &mut String, request: &SearchRequest<'_>) -> Vec<Value> {
    let mut values = Vec::new();
    if let Some(source) = request.source {
        sql.push_str(" AND c.source_id = ?");
        values.push(Value::Integer(source.value()));
    }
    if let Some(kind) = request.kind {
        sql.push_str(" AND c.kind = ?");
        values.push(Value::Text(kind_to_str(kind).to_owned()));
    }
    sql.push_str(
        " AND NOT EXISTS (SELECT 1 FROM hidden_channels h \
          WHERE h.source_id = c.source_id AND h.identity = c.identity)",
    );
    values
}

/// Runs a search: prefix first, trigram fallback second.
///
/// # Errors
/// Returns [`SearchError`] on a query or row-mapping failure.
pub fn search(conn: &Connection, request: &SearchRequest<'_>) -> SearchResult<SearchPage> {
    if let Some(match_expr) = query::compile_match(request.text) {
        let channels = run_prefix(conn, &match_expr, request)?;
        if !channels.is_empty() {
            return Ok(SearchPage {
                channels,
                fuzzy: false,
            });
        }
    }
    // Fuzzy fallback only when the prefix path found nothing and the query is substantial.
    if request.text.trim().chars().count() >= 3 {
        let channels = run_fuzzy(conn, request)?;
        if !channels.is_empty() {
            return Ok(SearchPage {
                channels,
                fuzzy: true,
            });
        }
    }
    Ok(SearchPage {
        channels: Vec::new(),
        fuzzy: false,
    })
}

fn run_prefix(
    conn: &Connection,
    match_expr: &str,
    request: &SearchRequest<'_>,
) -> SearchResult<Vec<Channel>> {
    let mut sql = format!(
        "SELECT {SELECT_COLUMNS} FROM channel_search s \
         JOIN channels c ON c.id = s.rowid WHERE channel_search MATCH ?"
    );
    let mut values = vec![Value::Text(match_expr.to_owned())];
    values.extend(push_filters(&mut sql, request));
    sql.push_str(" ORDER BY rank LIMIT ? OFFSET ?");
    values.push(Value::Integer(i64::from(request.limit)));
    values.push(Value::Integer(i64::from(request.offset)));

    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query(params_from_iter(values))?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(map_row(row)?);
    }
    Ok(out)
}

/// Fetches up to `budget` fuzzy candidates, drawn *round-robin across sources* so no single
/// source can monopolize the budget. A per-source `ROW_NUMBER()` window numbers each source's
/// rows; ordering by that rank (source id breaks ties) interleaves the sources, so the first
/// `budget` rows give every source an equal share — a channel in a later-imported source stays
/// reachable even when an earlier source is larger than the whole budget. The hard `LIMIT`
/// still bounds the worst case (PRD §9). With a single source (or `source` filtered to one),
/// this reduces to the first `budget` rows of that source, as before.
fn fuzzy_candidates(
    conn: &Connection,
    request: &SearchRequest<'_>,
    budget: usize,
) -> SearchResult<Vec<Channel>> {
    let mut inner = format!(
        "SELECT {SELECT_COLUMNS}, \
         ROW_NUMBER() OVER (PARTITION BY c.source_id ORDER BY c.id) AS rn \
         FROM channels c WHERE 1 = 1"
    );
    let mut values = push_filters(&mut inner, request);
    let sql =
        format!("SELECT {SELECT_COLUMNS} FROM ({inner}) c ORDER BY c.rn, c.source_id LIMIT ?");
    values.push(Value::Integer(i64::try_from(budget).unwrap_or(i64::MAX)));

    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query(params_from_iter(values))?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(map_row(row)?);
    }
    Ok(out)
}

fn run_fuzzy(conn: &Connection, request: &SearchRequest<'_>) -> SearchResult<Vec<Channel>> {
    let mut scored: Vec<(f32, Channel)> = Vec::new();
    for channel in fuzzy_candidates(conn, request, MAX_FUZZY_SCAN)? {
        // Score against the whole name and its best-matching word, so a typo on one word
        // of a long multi-word name is not diluted by the rest of the name.
        let score = channel
            .name
            .split_whitespace()
            .map(|word| ranking::similarity(request.text, word))
            .fold(ranking::similarity(request.text, &channel.name), f32::max);
        if score >= FUZZY_THRESHOLD {
            scored.push((score, channel));
        }
    }
    // Highest similarity first; ties keep stable insertion order.
    scored.sort_by(|a, b| b.0.total_cmp(&a.0));
    let offset = request.offset as usize;
    let limit = request.limit as usize;
    Ok(scored
        .into_iter()
        .skip(offset)
        .take(limit)
        .map(|(_, channel)| channel)
        .collect())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use core_db::{Db, NewChannel};
    use core_model::channel::channel_identity;
    use core_model::source::{Source, SourceCommon};

    fn channel(name: &str, group: &str) -> NewChannel {
        let url = format!("http://host/live/{}", name.replace(' ', "_"));
        NewChannel {
            identity: channel_identity(None, &url, name),
            name: name.to_owned(),
            group_title: Some(group.to_owned()),
            logo: None,
            locator: StreamLocator::parse(&url).unwrap(),
            kind: MediaKind::Live,
            category: None,
            overrides: ChannelOverrides::default(),
        }
    }

    fn seeded_db(names: &[(&str, &str)]) -> (Db, SourceId) {
        let db = Db::open_in_memory().unwrap();
        let source = {
            let conn = db.writer();
            core_db::repo::sources::insert(
                &conn,
                &Source::M3uFile {
                    id: SourceId::new(0),
                    common: SourceCommon {
                        name: "S".to_owned(),
                        enabled: true,
                        auto_refresh_secs: None,
                    },
                },
            )
            .unwrap()
        };
        let batch: Vec<NewChannel> = names.iter().map(|(n, g)| channel(n, g)).collect();
        let mut refresh = db.begin_staging(source).unwrap();
        refresh.stage(&batch).unwrap();
        refresh.commit(&db).unwrap();
        (db, source)
    }

    fn req(text: &str) -> SearchRequest<'_> {
        SearchRequest {
            text,
            source: None,
            kind: None,
            offset: 0,
            limit: 50,
        }
    }

    #[test]
    fn prefix_search_matches_by_name() {
        let (db, _) = seeded_db(&[
            ("BBC One HD", "News"),
            ("BBC Two HD", "News"),
            ("Discovery", "Docs"),
        ]);
        let conn = db.reader().unwrap();
        let page = search(&conn, &req("bbc")).unwrap();
        assert_eq!(page.channels.len(), 2);
        assert!(!page.fuzzy);
        assert!(page.channels.iter().all(|c| c.name.contains("BBC")));
    }

    #[test]
    fn prefix_search_is_multi_term_and() {
        let (db, _) = seeded_db(&[("BBC One HD", "News"), ("BBC Two HD", "News")]);
        let conn = db.reader().unwrap();
        let page = search(&conn, &req("bbc one")).unwrap();
        assert_eq!(page.channels.len(), 1);
        assert_eq!(page.channels[0].name, "BBC One HD");
    }

    #[test]
    fn hidden_channels_are_excluded() {
        let (db, source) = seeded_db(&[("BBC One HD", "News"), ("BBC Two HD", "News")]);
        let hide_identity = channel_identity(None, "http://host/live/BBC_One_HD", "BBC One HD");
        {
            let conn = db.writer();
            core_db::repo::channels::hide(&conn, source, hide_identity).unwrap();
        }
        let conn = db.reader().unwrap();
        let page = search(&conn, &req("bbc")).unwrap();
        assert_eq!(page.channels.len(), 1);
        assert_eq!(page.channels[0].name, "BBC Two HD");
    }

    #[test]
    fn fuzzy_fallback_recovers_typos() {
        let (db, _) = seeded_db(&[("Discovery Channel", "Docs"), ("Animal Planet", "Docs")]);
        let conn = db.reader().unwrap();
        // "discvoery" has no prefix match, so the fuzzy fallback should recover it.
        let page = search(&conn, &req("discvoery")).unwrap();
        assert!(page.fuzzy);
        assert_eq!(
            page.channels.first().map(|c| c.name.as_str()),
            Some("Discovery Channel")
        );
    }

    #[test]
    fn filters_restrict_by_kind() {
        let (db, source) = seeded_db(&[("BBC One HD", "News")]);
        let conn = db.reader().unwrap();
        let mut request = req("bbc");
        request.source = Some(source);
        request.kind = Some(MediaKind::Movie); // no movies seeded
        assert!(search(&conn, &request).unwrap().channels.is_empty());
        request.kind = Some(MediaKind::Live);
        assert_eq!(search(&conn, &request).unwrap().channels.len(), 1);
    }

    fn add_source(db: &Db, name: &str, chans: &[(&str, &str)]) -> SourceId {
        let source = {
            let conn = db.writer();
            core_db::repo::sources::insert(
                &conn,
                &Source::M3uFile {
                    id: SourceId::new(0),
                    common: SourceCommon {
                        name: name.to_owned(),
                        enabled: true,
                        auto_refresh_secs: None,
                    },
                },
            )
            .unwrap()
        };
        let batch: Vec<NewChannel> = chans.iter().map(|(n, g)| channel(n, g)).collect();
        let mut refresh = db.begin_staging(source).unwrap();
        refresh.stage(&batch).unwrap();
        refresh.commit(db).unwrap();
        source
    }

    #[test]
    fn fuzzy_scan_is_fair_across_sources() {
        // Two sources, second imported after the first. With a budget no larger than the
        // first source's channel count, a naive rowid-order `LIMIT budget` scan spends the
        // whole budget on the first source and never reaches the second — hiding its
        // channels from typo search though they are present and visible. The round-robin
        // scan interleaves sources, so the later source stays reachable within budget.
        let db = Db::open_in_memory().unwrap();
        let _a = add_source(&db, "A", &[("Alpha One", "A"), ("Alpha Two", "A")]);
        let b = add_source(&db, "B", &[("Beta One", "B")]);

        let conn = db.reader().unwrap();
        // Budget 2 == source A's row count: an unordered scan would return only A's rows.
        let candidates = fuzzy_candidates(&conn, &req("beta"), 2).unwrap();
        assert!(
            candidates.iter().any(|c| c.source_id.value() == b.value()),
            "round-robin scan must reach the later source within the budget"
        );
    }
}
