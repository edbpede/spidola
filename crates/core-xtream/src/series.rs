// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Series → seasons/episodes expansion (TECH_SPEC §4.3).
//!
//! Series are the one Xtream content type that is not a flat list. `get_series` returns
//! *shows*, which are not playable and have no stream id; only `get_series_info` for a
//! given show yields the episodes that are. So the two live here rather than in
//! `crate::catalog`: [`list`] enumerates the shows, and [`expand`] turns one show into its
//! episodes, which are ordinary [`CatalogChannel`]s of kind `SeriesEpisode`.
//!
//! Season numbers are reconstructed rather than trusted — see [`Expansion::episodes`] and
//! [`EpisodeRow::season`] for why that is not the same as reading a field.

use core_fetch::HttpClient;
use core_model::channel::MediaKind;
use core_model::secret::Secret;
use serde_json::Value;

use crate::LOG_TARGET;
use crate::catalog::CatalogChannel;
use crate::diagnostics::{Diagnostics, SkipReason};
use crate::error::XtreamResult;
use crate::request;
use crate::urls::{Endpoint, StreamRef};
use crate::wire::{self, EpisodeDto, SeasonBucket, SeriesDto, SeriesInfoDto};

/// The season assumed for an episode no one placed in one.
///
/// A show whose episodes carry neither a season key nor a season field is almost always a
/// single-season show a panel forgot to label. Calling that season 1 renders correctly;
/// calling it 0 invents a season that does not exist.
const DEFAULT_SEASON: u32 = 1;

/// A show, as `get_series` lists it. Not playable — [`expand`] turns it into episodes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Show {
    /// The headend's id, the argument to [`expand`].
    pub series_id: u64,
    /// Display name.
    pub name: String,
    /// Cover artwork, if supplied.
    pub cover: Option<String>,
    /// The headend's category id, resolved by `core-api` as for any other row.
    pub category_key: Option<String>,
}

/// One episode: a catalog row plus the position that row cannot carry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EpisodeRow {
    /// The playable row, ready for `core-db` exactly like a live or VOD channel.
    pub channel: CatalogChannel,
    /// Which season this episode belongs to.
    ///
    /// Reconstructed: the map key wins where the response had one (it is the headend's own
    /// grouping), then the episode's own `season` field, then [`DEFAULT_SEASON`]. Panels
    /// routinely disagree with themselves here — a row keyed under `"2"` while its own
    /// field says `0` is common — and the key is the more reliable of the two.
    pub season: u32,
    /// The episode's number within its season, if stated.
    pub episode: Option<u32>,
}

/// One show's episodes, with the ledger of what was dropped.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Expansion {
    /// The show's name, as `get_series_info` reports it. Becomes each episode's group
    /// label, so the browse axis reads source → type → series → episode.
    pub series_name: Option<String>,
    /// Episodes, ordered by season then episode number so a UI can render them directly.
    pub episodes: Vec<EpisodeRow>,
    /// What was skipped and why.
    pub diagnostics: Diagnostics,
}

/// Fetches the series listing, optionally limited to one category.
///
/// # Errors
/// Returns [`crate::XtreamError::Malformed`] if the response is not a list, or a
/// [`crate::XtreamError::Transport`] failure if the request never landed.
pub async fn list(
    http: &HttpClient,
    endpoint: &Endpoint,
    password: &Secret,
    category: Option<&str>,
) -> XtreamResult<Vec<Show>> {
    let mut params = vec![("action", "get_series")];
    if let Some(id) = category {
        params.push(("category_id", id));
    }
    let body = request::get(http, endpoint, password, &params).await?;
    let rows = wire::parse_rows(&body)?;
    Ok(map_shows(rows))
}

/// Expands one show into its episodes.
///
/// # Errors
/// Returns [`crate::XtreamError::Malformed`] if the response is not a series-info object,
/// or a [`crate::XtreamError::Transport`] failure if the request never landed.
pub async fn expand(
    http: &HttpClient,
    endpoint: &Endpoint,
    password: &Secret,
    series_id: u64,
) -> XtreamResult<Expansion> {
    let series = series_id.to_string();
    let body = request::get(
        http,
        endpoint,
        password,
        &[("action", "get_series_info"), ("series_id", &series)],
    )
    .await?;
    let info: SeriesInfoDto = wire::parse_object(&body)?;
    Ok(map_expansion(endpoint, series_id, info))
}

/// Maps show rows, skipping any that lack an id or a name.
fn map_shows(rows: Vec<Value>) -> Vec<Show> {
    let total = rows.len();
    let shows: Vec<Show> = rows
        .into_iter()
        .filter_map(|row| {
            let dto: SeriesDto = serde_json::from_value(row).ok()?;
            Some(Show {
                series_id: dto.series_id.filter(|id| *id != 0)?,
                name: dto.name?,
                cover: dto.cover,
                category_key: dto.category_id,
            })
        })
        .collect();
    if shows.len() != total {
        tracing::warn!(
            target: LOG_TARGET,
            skipped = total - shows.len(),
            "dropped unusable series rows"
        );
    }
    shows
}

/// Maps a `get_series_info` response into ordered episodes.
fn map_expansion(endpoint: &Endpoint, series_id: u64, info: SeriesInfoDto) -> Expansion {
    let meta = info.info;
    let series_name = meta.as_ref().and_then(|m| m.name.clone());
    let series_cover = meta.and_then(|m| m.cover);

    let mut diagnostics = Diagnostics::default();
    let mut episodes = Vec::new();
    for bucket in info.episodes {
        let SeasonBucket { season, rows } = bucket;
        for row in rows {
            if let Some(episode) = map_episode(
                endpoint,
                series_id,
                season,
                series_name.as_deref(),
                series_cover.as_deref(),
                row,
                &mut diagnostics,
            ) {
                episodes.push(episode);
            }
        }
    }
    // A keyed response arrives in map order, and the array shape carries no order at all,
    // so ordering is established here rather than inherited.
    episodes.sort_by_key(|e| (e.season, e.episode));

    if diagnostics.skipped() > 0 {
        tracing::warn!(
            target: LOG_TARGET,
            series_id,
            seen = diagnostics.total_seen(),
            skipped = diagnostics.skipped(),
            "skipped unusable episode rows"
        );
    }
    Expansion {
        series_name,
        episodes,
        diagnostics,
    }
}

/// Maps one episode, recording exactly one diagnostic for it either way.
#[allow(
    clippy::too_many_arguments,
    reason = "one concept: everything an episode row needs from its show to become a \
              catalog row. Bundling them into a struct would be a type that exists only to \
              satisfy the lint (doctrine §3.1)."
)]
fn map_episode(
    endpoint: &Endpoint,
    series_id: u64,
    season_key: Option<u32>,
    series_name: Option<&str>,
    series_cover: Option<&str>,
    row: Value,
    diagnostics: &mut Diagnostics,
) -> Option<EpisodeRow> {
    let Ok(dto) = serde_json::from_value::<EpisodeDto>(row) else {
        diagnostics.record_skip(SkipReason::MalformedEntry);
        return None;
    };
    let Some(episode_id) = dto.id.filter(|id| *id != 0) else {
        diagnostics.record_skip(SkipReason::MissingId);
        return None;
    };
    let Some(stream) = StreamRef::new(
        MediaKind::SeriesEpisode,
        episode_id,
        dto.container_extension.as_deref(),
    ) else {
        diagnostics.record_skip(SkipReason::UnusableExtension);
        return None;
    };
    let Ok(locator) = stream.to_catalog_locator(endpoint.server()) else {
        diagnostics.record_skip(SkipReason::InvalidLocator);
        return None;
    };

    let season = season_key
        .or_else(|| dto.season.and_then(|s| u32::try_from(s).ok()))
        .unwrap_or(DEFAULT_SEASON);
    let episode = dto.episode_num.and_then(|n| u32::try_from(n).ok());
    // An untitled episode is still watchable, so — unlike a live channel — it earns a
    // derived name rather than a skip. `S02E05` is what a UI would render anyway.
    let name = dto.title.unwrap_or_else(|| match episode {
        Some(number) => format!("S{season:02}E{number:02}"),
        None => format!("S{season:02}"),
    });

    diagnostics.record_emitted();
    Some(EpisodeRow {
        channel: CatalogChannel {
            identity: identity_key(series_id, episode_id, &locator, &name),
            epg_key: None,
            name,
            // The show is the group: browse reads source → type → series → episode.
            group_title: series_name.map(str::to_owned),
            logo: dto
                .info
                .and_then(|i| i.movie_image)
                .or_else(|| series_cover.map(str::to_owned)),
            locator,
            kind: MediaKind::SeriesEpisode,
            // Episodes inherit their show's category through the show, not the row: the
            // per-episode payload has no `category_id`.
            category_key: None,
        },
        season,
        episode,
    })
}

/// Derives an episode's stable identity.
///
/// Namespaced by `series_id` as well as episode id: episode ids are unique per headend in
/// practice, but a show is the unit a user re-adds or a panel re-imports, and pinning the
/// identity to the pair keeps an episode stable exactly as long as its show is. Same
/// rationale as `crate::catalog`'s key — see that function for why the id and not the
/// name.
fn identity_key(
    series_id: u64,
    episode_id: u64,
    locator: &core_model::locator::StreamLocator,
    name: &str,
) -> core_model::ids::ChannelIdentity {
    let key = format!("xtream:series:{series_id}:{episode_id}");
    core_model::channel::channel_identity(Some(&key), locator.as_str(), name)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use core_model::locator::StreamLocator;

    fn endpoint() -> Endpoint {
        Endpoint::new(
            &StreamLocator::parse("http://panel.example:8080").unwrap(),
            "alice",
        )
        .unwrap()
    }

    fn expand_json(json: &str) -> Expansion {
        let info: SeriesInfoDto = wire::parse_object(json.as_bytes()).unwrap();
        map_expansion(&endpoint(), 55, info)
    }

    // ---- The two episode shapes -------------------------------------------------------

    #[test]
    fn the_keyed_object_shape_expands_with_its_season_numbers() {
        let expansion = expand_json(
            r#"{
                "info": {"name": "Some Show", "cover": "http://cdn.example/show.jpg"},
                "episodes": {
                    "1": [
                        {"id": "101", "episode_num": 1, "title": "Pilot",
                         "container_extension": "mkv"},
                        {"id": "102", "episode_num": 2, "title": "Second"}
                    ],
                    "2": [{"id": "201", "episode_num": 1, "title": "Return"}]
                }
            }"#,
        );
        assert_eq!(expansion.series_name.as_deref(), Some("Some Show"));
        assert_eq!(expansion.episodes.len(), 3);
        assert_eq!(expansion.diagnostics.emitted(), 3);

        let first = &expansion.episodes[0];
        assert_eq!(first.season, 1);
        assert_eq!(first.episode, Some(1));
        assert_eq!(first.channel.name, "Pilot");
        assert_eq!(first.channel.kind, MediaKind::SeriesEpisode);
        // The show is the browse group, and the locator is credential-free.
        assert_eq!(first.channel.group_title.as_deref(), Some("Some Show"));
        assert_eq!(
            first.channel.locator.as_str(),
            "http://panel.example:8080/series/101.mkv"
        );
        // No container_extension → the VOD default, not the live one.
        assert_eq!(
            expansion.episodes[1].channel.locator.as_str(),
            "http://panel.example:8080/series/102.mp4"
        );
        assert_eq!(expansion.episodes[2].season, 2);
    }

    #[test]
    fn the_array_shape_falls_back_to_each_episodes_own_season() {
        // No keys to read, so the season must come from the rows themselves.
        let expansion = expand_json(
            r#"{
                "info": {"name": "Array Show"},
                "episodes": [
                    [{"id": 1, "season": 1, "episode_num": 1, "title": "A"}],
                    [{"id": 2, "season": 2, "episode_num": 1, "title": "B"}]
                ]
            }"#,
        );
        assert_eq!(expansion.episodes.len(), 2);
        assert_eq!(expansion.episodes[0].season, 1);
        assert_eq!(expansion.episodes[1].season, 2);
    }

    #[test]
    fn the_season_key_outranks_a_disagreeing_row() {
        // Panels commonly key an episode under "2" while its own field still says 0.
        let expansion = expand_json(
            r#"{"episodes": {"2": [{"id": 1, "season": 0, "episode_num": 5, "title": "X"}]}}"#,
        );
        assert_eq!(expansion.episodes[0].season, 2, "the key is authoritative");
    }

    #[test]
    fn an_unplaced_episode_lands_in_season_one() {
        let expansion = expand_json(r#"{"episodes": [[{"id": 1, "title": "Only"}]]}"#);
        assert_eq!(expansion.episodes[0].season, DEFAULT_SEASON);
        assert_eq!(expansion.episodes[0].episode, None);
    }

    // ---- Ordering and naming ------------------------------------------------------------

    #[test]
    fn episodes_come_back_ordered_by_season_then_episode() {
        // JSON object key order is not season order, so the mapper must impose it.
        let expansion = expand_json(
            r#"{"episodes": {
                "10": [{"id": 3, "episode_num": 1, "title": "S10E1"}],
                "2":  [{"id": 2, "episode_num": 2, "title": "S2E2"},
                       {"id": 1, "episode_num": 1, "title": "S2E1"}]
            }}"#,
        );
        let order: Vec<(u32, Option<u32>)> = expansion
            .episodes
            .iter()
            .map(|e| (e.season, e.episode))
            .collect();
        assert_eq!(order, [(2, Some(1)), (2, Some(2)), (10, Some(1))]);
    }

    #[test]
    fn an_untitled_episode_earns_a_derived_name_rather_than_a_skip() {
        let expansion = expand_json(r#"{"episodes": {"2": [{"id": 1, "episode_num": 5}]}}"#);
        assert_eq!(expansion.episodes[0].channel.name, "S02E05");
        assert_eq!(expansion.diagnostics.skipped(), 0);
    }

    #[test]
    fn episode_artwork_prefers_its_own_still_over_the_show_cover() {
        let expansion = expand_json(
            r#"{
                "info": {"name": "Show", "cover": "http://cdn.example/cover.jpg"},
                "episodes": {"1": [
                    {"id": 1, "title": "Has Still",
                     "info": {"movie_image": "http://cdn.example/still.jpg"}},
                    {"id": 2, "title": "No Still"}
                ]}
            }"#,
        );
        assert_eq!(
            expansion.episodes[0].channel.logo.as_deref(),
            Some("http://cdn.example/still.jpg")
        );
        assert_eq!(
            expansion.episodes[1].channel.logo.as_deref(),
            Some("http://cdn.example/cover.jpg"),
            "an episode with no still falls back to the show cover"
        );
    }

    // ---- Tolerance ------------------------------------------------------------------------

    #[test]
    fn one_unusable_episode_never_costs_the_season() {
        let expansion = expand_json(
            r#"{"episodes": {"1": [
                {"id": 1, "title": "Good"},
                {"id": 0, "title": "Zero Id"},
                {"title": "No Id"},
                {"id": 4, "title": "Bad Ext", "container_extension": "!!"},
                {"id": [], "title": "Wrong Shape"},
                {"id": 6, "title": "Also Good"}
            ]}}"#,
        );
        let names: Vec<&str> = expansion
            .episodes
            .iter()
            .map(|e| e.channel.name.as_str())
            .collect();
        assert_eq!(names, ["Good", "Also Good"]);

        let d = &expansion.diagnostics;
        assert_eq!(d.total_seen(), 6);
        assert_eq!(d.emitted(), 2);
        assert!(d.is_balanced());
        assert_eq!(d.skips_for(SkipReason::MissingId), 2);
        assert_eq!(d.skips_for(SkipReason::UnusableExtension), 1);
        assert_eq!(d.skips_for(SkipReason::MalformedEntry), 1);
    }

    #[test]
    fn a_show_with_no_episodes_is_empty_not_an_error() {
        let expansion = expand_json(r#"{"info": {"name": "Announced"}, "episodes": {}}"#);
        assert!(expansion.episodes.is_empty());
        assert_eq!(expansion.diagnostics.total_seen(), 0);
        assert!(expansion.diagnostics.is_balanced());
    }

    // ---- Identity ---------------------------------------------------------------------------

    #[test]
    fn episode_identities_are_distinct_and_survive_a_retitle() {
        let before = expand_json(r#"{"episodes": {"1": [{"id": 1}, {"id": 2}]}}"#);
        let after = expand_json(r#"{"episodes": {"1": [{"id": 1, "title": "Now Titled"}]}}"#);
        assert_ne!(
            before.episodes[0].channel.identity, before.episodes[1].channel.identity,
            "two episodes must not share an identity"
        );
        assert_eq!(
            before.episodes[0].channel.identity, after.episodes[0].channel.identity,
            "a retitled episode is the same episode (§4.4)"
        );
    }

    // ---- The show listing --------------------------------------------------------------------

    #[test]
    fn show_rows_map_and_drop_the_unusable() {
        let shows = map_shows(
            wire::parse_rows(
                br#"[
                    {"series_id": 1, "name": "Alpha", "cover": "http://cdn.example/a.jpg",
                     "category_id": "7"},
                    {"series_id": "2", "name": "Beta"},
                    {"series_id": 0, "name": "Zero Id"},
                    {"name": "No Id"},
                    {"series_id": 5}
                ]"#,
            )
            .unwrap(),
        );
        assert_eq!(
            shows,
            vec![
                Show {
                    series_id: 1,
                    name: "Alpha".to_owned(),
                    cover: Some("http://cdn.example/a.jpg".to_owned()),
                    category_key: Some("7".to_owned()),
                },
                Show {
                    series_id: 2,
                    name: "Beta".to_owned(),
                    cover: None,
                    category_key: None,
                },
            ]
        );
    }
}
