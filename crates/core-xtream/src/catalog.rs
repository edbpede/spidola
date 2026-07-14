// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Live / VOD / category listings, mapped into domain rows (TECH_SPEC §4.3).
//!
//! The three listing endpoints differ only in their action name and their `MediaKind`, so
//! one mapping serves all of them. What it produces is deliberately *not* Xtream-shaped:
//! [`CatalogCategory`] and [`CatalogChannel`] are `core-model` values minus the rowids only
//! `core-db` can mint, so `core-api` finishes them by assigning ids and nothing downstream
//! ever learns this source was an Xtream account.
//!
//! Tolerance follows `core-parse`'s rule exactly (§4.2): the envelope must be the list the
//! endpoint promised or the call fails, but a row inside it that cannot be mapped is
//! skipped and counted, so one unusable title never costs a user their catalog.

use std::collections::BTreeMap;

use core_fetch::HttpClient;
use core_model::channel::{MediaKind, channel_identity};
use core_model::ids::ChannelIdentity;
use core_model::locator::StreamLocator;
use core_model::secret::Secret;
use serde_json::Value;

use crate::LOG_TARGET;
use crate::diagnostics::{Diagnostics, SkipReason};
use crate::error::XtreamResult;
use crate::request;
use crate::urls::{Endpoint, StreamRef};
use crate::wire::{self, CategoryDto, StreamDto};

/// A browse grouping, ready for `core-db` to assign it a rowid.
///
/// Mirrors `core_model::Category` without `id`/`source_id`; [`Self::remote_id`] is the field
/// that type documents as "the source's own id … (Xtream `category_id`)".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogCategory {
    /// Which content type this category groups.
    pub kind: MediaKind,
    /// Display name.
    pub name: String,
    /// The headend's own id, used to re-associate channels on refresh.
    pub remote_id: String,
}

/// A catalog row, ready for `core-db` to assign it a rowid.
///
/// Mirrors `core_db::NewChannel` field for field, except that [`Self::category_key`] is the
/// headend's category id rather than a resolved `CategoryId` — this crate cannot mint
/// rowids, so `core-api` performs that join. Per-channel overrides are absent because
/// Xtream needs none: its auth rides in the URL, not in headers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogChannel {
    /// Stable per-source identity (see [`identity_key`]).
    pub identity: ChannelIdentity,
    /// Display name.
    pub name: String,
    /// Group label — the channel's category name, when its category is known.
    pub group_title: Option<String>,
    /// Artwork URL, if the headend supplied one.
    pub logo: Option<String>,
    /// **Credential-free** locator (`crate::urls`): what gets persisted. Resolve it through
    /// `Endpoint::resolve_stream` at playback time to obtain the playable URL (§12).
    pub locator: StreamLocator,
    /// What the channel plays.
    pub kind: MediaKind,
    /// The headend's category id, for `core-api` to resolve against [`CatalogCategory`].
    pub category_key: Option<String>,
}

/// Rows mapped from one listing, with the ledger of what was dropped along the way.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Listing {
    /// The rows that mapped cleanly, in the order the headend returned them.
    pub channels: Vec<CatalogChannel>,
    /// What was skipped and why. `total_seen` counts every row the headend sent.
    pub diagnostics: Diagnostics,
}

/// Fetches the categories for one content type.
///
/// `kind` selects the endpoint: live, VOD (`Movie`), or series (`SeriesEpisode`).
///
/// # Errors
/// Returns [`crate::XtreamError::Malformed`] if the response is not a list, or a
/// [`crate::XtreamError::Transport`] failure if the request never landed.
pub async fn categories(
    http: &HttpClient,
    endpoint: &Endpoint,
    password: &Secret,
    kind: MediaKind,
) -> XtreamResult<Vec<CatalogCategory>> {
    let action = match kind {
        MediaKind::Live => "get_live_categories",
        MediaKind::Movie => "get_vod_categories",
        MediaKind::SeriesEpisode => "get_series_categories",
    };
    let body = request::get(http, endpoint, password, &[("action", action)]).await?;
    let rows = wire::parse_rows(&body)?;
    Ok(map_categories(kind, rows))
}

/// Fetches the live channel listing, optionally limited to one category.
///
/// `categories` supplies the group labels; pass the result of [`categories`] for the same
/// kind. An empty slice is legal and simply leaves every `group_title` unset.
///
/// # Errors
/// As [`categories`].
pub async fn live_streams(
    http: &HttpClient,
    endpoint: &Endpoint,
    password: &Secret,
    categories: &[CatalogCategory],
    category: Option<&str>,
) -> XtreamResult<Listing> {
    streams(
        http,
        endpoint,
        password,
        categories,
        category,
        MediaKind::Live,
        "get_live_streams",
    )
    .await
}

/// Fetches the VOD listing, optionally limited to one category.
///
/// # Errors
/// As [`categories`].
pub async fn vod_streams(
    http: &HttpClient,
    endpoint: &Endpoint,
    password: &Secret,
    categories: &[CatalogCategory],
    category: Option<&str>,
) -> XtreamResult<Listing> {
    streams(
        http,
        endpoint,
        password,
        categories,
        category,
        MediaKind::Movie,
        "get_vod_streams",
    )
    .await
}

/// The shared body of [`live_streams`] and [`vod_streams`].
async fn streams(
    http: &HttpClient,
    endpoint: &Endpoint,
    password: &Secret,
    categories: &[CatalogCategory],
    category: Option<&str>,
    kind: MediaKind,
    action: &str,
) -> XtreamResult<Listing> {
    let mut params = vec![("action", action)];
    if let Some(id) = category {
        params.push(("category_id", id));
    }
    let body = request::get(http, endpoint, password, &params).await?;
    let rows = wire::parse_rows(&body)?;
    Ok(map_streams(endpoint, kind, rows, categories))
}

/// Maps category rows, skipping any that lack an id or a name.
fn map_categories(kind: MediaKind, rows: Vec<Value>) -> Vec<CatalogCategory> {
    let total = rows.len();
    let mapped: Vec<CatalogCategory> = rows
        .into_iter()
        .filter_map(|row| {
            let dto: CategoryDto = serde_json::from_value(row).ok()?;
            Some(CatalogCategory {
                kind,
                name: dto.category_name?,
                remote_id: dto.category_id?,
            })
        })
        .collect();
    if mapped.len() != total {
        tracing::warn!(
            target: LOG_TARGET,
            skipped = total - mapped.len(),
            "dropped unusable category rows"
        );
    }
    mapped
}

/// Maps stream rows into catalog channels, accounting for every row it drops.
fn map_streams(
    endpoint: &Endpoint,
    kind: MediaKind,
    rows: Vec<Value>,
    categories: &[CatalogCategory],
) -> Listing {
    let labels = label_index(categories);
    let mut diagnostics = Diagnostics::default();
    let mut channels = Vec::with_capacity(rows.len());
    for row in rows {
        if let Some(channel) = map_stream(endpoint, kind, row, &labels, &mut diagnostics) {
            channels.push(channel);
        }
    }
    if diagnostics.skipped() > 0 {
        tracing::warn!(
            target: LOG_TARGET,
            seen = diagnostics.total_seen(),
            skipped = diagnostics.skipped(),
            "skipped unusable catalog rows"
        );
    }
    Listing {
        channels,
        diagnostics,
    }
}

/// Maps one row, recording exactly one diagnostic for it either way.
fn map_stream(
    endpoint: &Endpoint,
    kind: MediaKind,
    row: Value,
    labels: &BTreeMap<&str, &str>,
    diagnostics: &mut Diagnostics,
) -> Option<CatalogChannel> {
    let Ok(dto) = serde_json::from_value::<StreamDto>(row) else {
        diagnostics.record_skip(SkipReason::MalformedEntry);
        return None;
    };
    // Xtream spells "no id" as absent, null, "", and 0; all four mean unplayable.
    let Some(stream_id) = dto.stream_id.filter(|id| *id != 0) else {
        diagnostics.record_skip(SkipReason::MissingId);
        return None;
    };
    let Some(name) = dto.name else {
        diagnostics.record_skip(SkipReason::MissingName);
        return None;
    };
    let Some(stream) = StreamRef::new(kind, stream_id, dto.container_extension.as_deref()) else {
        diagnostics.record_skip(SkipReason::UnusableExtension);
        return None;
    };
    let Ok(locator) = stream.to_catalog_locator(endpoint.server()) else {
        diagnostics.record_skip(SkipReason::InvalidLocator);
        return None;
    };

    let group_title = dto
        .category_id
        .as_deref()
        .and_then(|id| labels.get(id))
        .map(|name| (*name).to_owned());
    diagnostics.record_emitted();
    Some(CatalogChannel {
        identity: identity_key(kind, stream_id, &locator, &name),
        name,
        group_title,
        logo: dto.stream_icon,
        locator,
        kind,
        category_key: dto.category_id,
    })
}

/// Category display names by remote id.
fn label_index(categories: &[CatalogCategory]) -> BTreeMap<&str, &str> {
    categories
        .iter()
        .map(|c| (c.remote_id.as_str(), c.name.as_str()))
        .collect()
}

/// Derives a channel's stable identity from its `stream_id`.
///
/// `channel_identity`'s first argument is the "most durable attribute" slot — `tvg-id` for
/// M3U. Xtream's equivalent is emphatically **not** `epg_channel_id`, which is blank on
/// most rows and *shared* between the SD, HD, and 4K variants of one channel; using it
/// would collide those three into one identity and the refresh would drop two of them as
/// duplicates. The `stream_id` is the headend's own primary key: unique per row and stable
/// across refreshes, which is precisely what favorites and hidden flags need (§4.4). It is
/// namespaced by kind so a live stream and a movie sharing an id stay distinct.
///
/// The locator and name are passed through only as `channel_identity`'s fallbacks; with a
/// key always present they are never consulted, which is what makes the identity survive a
/// server-address change or a rename.
fn identity_key(
    kind: MediaKind,
    stream_id: u64,
    locator: &StreamLocator,
    name: &str,
) -> ChannelIdentity {
    let kind_label = match kind {
        MediaKind::Live => "live",
        MediaKind::Movie => "movie",
        MediaKind::SeriesEpisode => "series",
    };
    let key = format!("xtream:{kind_label}:{stream_id}");
    channel_identity(Some(&key), locator.as_str(), name)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    fn endpoint() -> Endpoint {
        Endpoint::new(
            &StreamLocator::parse("http://panel.example:8080").unwrap(),
            "alice",
        )
        .unwrap()
    }

    fn rows(json: &str) -> Vec<Value> {
        wire::parse_rows(json.as_bytes()).unwrap()
    }

    fn news() -> Vec<CatalogCategory> {
        vec![CatalogCategory {
            kind: MediaKind::Live,
            name: "News".to_owned(),
            remote_id: "1".to_owned(),
        }]
    }

    // ---- Envelope vs entry ----------------------------------------------------------

    #[test]
    fn a_non_list_envelope_is_a_typed_error() {
        // The contract says "array". An object is a broken endpoint, not a broken row.
        for body in [
            r#"{"user_info": {}}"#,
            "null",
            "\"nope\"",
            "not json at all",
        ] {
            assert!(
                wire::parse_rows(body.as_bytes()).is_err(),
                "{body} should fail the envelope"
            );
        }
    }

    #[test]
    fn one_unusable_row_never_costs_the_others() {
        // The core tolerance law: a mixed listing yields every good row and accounts for
        // each bad one. This is what stops a single title failing a 50k import.
        let listing = map_streams(
            &endpoint(),
            MediaKind::Live,
            rows(
                r#"[
                    {"stream_id": 1, "name": "Good One", "category_id": "1"},
                    {"stream_id": "not a number", "name": "No Id"},
                    {"stream_id": 0, "name": "Zero Id"},
                    {"stream_id": 4, "name": ""},
                    {"stream_id": 5, "name": "Bad Ext", "container_extension": "m p 4"},
                    {"stream_id": [], "name": "Wrong Shape"},
                    {"stream_id": 7, "name": "Good Two", "category_id": "1"}
                ]"#,
            ),
            &news(),
        );

        let names: Vec<&str> = listing.channels.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["Good One", "Good Two"], "good rows must survive");

        let d = &listing.diagnostics;
        assert_eq!(d.total_seen(), 7);
        assert_eq!(d.emitted(), 2);
        assert_eq!(d.skipped(), 5);
        assert!(
            d.is_balanced(),
            "every row must be emitted or accounted for"
        );
        assert_eq!(d.skips_for(SkipReason::MissingId), 2, "\"not a number\", 0");
        assert_eq!(d.skips_for(SkipReason::MissingName), 1);
        assert_eq!(d.skips_for(SkipReason::UnusableExtension), 1);
        assert_eq!(d.skips_for(SkipReason::MalformedEntry), 1);
    }

    // ---- The mapping ------------------------------------------------------------------

    #[test]
    fn a_live_row_maps_onto_the_domain() {
        let listing = map_streams(
            &endpoint(),
            MediaKind::Live,
            rows(
                r#"[{
                    "num": 1, "name": "BBC One HD", "stream_type": "live",
                    "stream_id": 4242, "stream_icon": "http://cdn.example/bbc1.png",
                    "epg_channel_id": "bbc.one", "category_id": "1", "tv_archive": 0
                }]"#,
            ),
            &news(),
        );
        let channel = &listing.channels[0];
        assert_eq!(channel.name, "BBC One HD");
        assert_eq!(channel.kind, MediaKind::Live);
        assert_eq!(channel.logo.as_deref(), Some("http://cdn.example/bbc1.png"));
        assert_eq!(channel.category_key.as_deref(), Some("1"));
        // The category name becomes the browse label (Xtream's analogue of `group-title`).
        assert_eq!(channel.group_title.as_deref(), Some("News"));
        // Live streams default to `ts`, and the persisted locator holds no credentials.
        assert_eq!(
            channel.locator.as_str(),
            "http://panel.example:8080/live/4242.ts"
        );
    }

    #[test]
    fn a_vod_row_honours_its_container_extension() {
        let listing = map_streams(
            &endpoint(),
            MediaKind::Movie,
            rows(r#"[{"stream_id": "99", "name": "Some Film", "container_extension": "mkv"}]"#),
            &[],
        );
        assert_eq!(
            listing.channels[0].locator.as_str(),
            "http://panel.example:8080/movie/99.mkv"
        );
        assert_eq!(listing.channels[0].kind, MediaKind::Movie);
    }

    #[test]
    fn a_row_in_an_unknown_category_keeps_its_key_but_has_no_label() {
        let listing = map_streams(
            &endpoint(),
            MediaKind::Live,
            rows(r#"[{"stream_id": 1, "name": "Orphan", "category_id": "999"}]"#),
            &news(),
        );
        // The key survives for `core-api` to resolve; only the label is unknown.
        assert_eq!(listing.channels[0].category_key.as_deref(), Some("999"));
        assert_eq!(listing.channels[0].group_title, None);
    }

    #[test]
    fn rows_keep_the_order_the_headend_sent() {
        // `core-db` derives `sort_index` positionally, so order is data, not incidental.
        let listing = map_streams(
            &endpoint(),
            MediaKind::Live,
            rows(
                r#"[{"stream_id":3,"name":"C"},{"stream_id":1,"name":"A"},{"stream_id":2,"name":"B"}]"#,
            ),
            &[],
        );
        let names: Vec<&str> = listing.channels.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["C", "A", "B"]);
    }

    // ---- Identity ----------------------------------------------------------------------

    #[test]
    fn identity_survives_a_rename_and_a_server_move() {
        let moved = Endpoint::new(
            &StreamLocator::parse("https://new-panel.example").unwrap(),
            "alice",
        )
        .unwrap();
        let before = map_streams(
            &endpoint(),
            MediaKind::Live,
            rows(r#"[{"stream_id": 4242, "name": "BBC One"}]"#),
            &[],
        );
        let after = map_streams(
            &moved,
            MediaKind::Live,
            rows(r#"[{"stream_id": 4242, "name": "BBC One HD ⁴ᴷ"}]"#),
            &[],
        );
        assert_eq!(
            before.channels[0].identity, after.channels[0].identity,
            "favorites must survive a rename and a server address change (§4.4)"
        );
    }

    #[test]
    fn quality_variants_sharing_an_epg_id_stay_distinct() {
        // The reason identity keys on `stream_id`: these three rows share `epg_channel_id`,
        // and keying on that would collide them into one, losing two to the refresh's
        // duplicate drop.
        let listing = map_streams(
            &endpoint(),
            MediaKind::Live,
            rows(
                r#"[
                    {"stream_id": 1, "name": "BBC One SD", "epg_channel_id": "bbc.one"},
                    {"stream_id": 2, "name": "BBC One HD", "epg_channel_id": "bbc.one"},
                    {"stream_id": 3, "name": "BBC One 4K", "epg_channel_id": "bbc.one"}
                ]"#,
            ),
            &[],
        );
        let mut identities: Vec<u64> = listing
            .channels
            .iter()
            .map(|c| c.identity.value())
            .collect();
        identities.sort_unstable();
        identities.dedup();
        assert_eq!(identities.len(), 3, "quality variants collided into one");
    }

    #[test]
    fn the_same_id_under_different_kinds_stays_distinct() {
        let live = map_streams(
            &endpoint(),
            MediaKind::Live,
            rows(r#"[{"stream_id": 7, "name": "Seven"}]"#),
            &[],
        );
        let movie = map_streams(
            &endpoint(),
            MediaKind::Movie,
            rows(r#"[{"stream_id": 7, "name": "Seven"}]"#),
            &[],
        );
        assert_ne!(live.channels[0].identity, movie.channels[0].identity);
    }

    // ---- Categories ----------------------------------------------------------------------

    #[test]
    fn category_rows_map_and_tolerate_both_id_spellings() {
        let mapped = map_categories(
            MediaKind::Live,
            rows(
                r#"[
                    {"category_id": "1", "category_name": "News", "parent_id": 0},
                    {"category_id": 2, "category_name": "Sport", "parent_id": 0},
                    {"category_id": "3"},
                    {"category_name": "No Id"},
                    {"category_id": "", "category_name": "Blank Id"}
                ]"#,
            ),
        );
        assert_eq!(
            mapped,
            vec![
                CatalogCategory {
                    kind: MediaKind::Live,
                    name: "News".to_owned(),
                    remote_id: "1".to_owned(),
                },
                CatalogCategory {
                    kind: MediaKind::Live,
                    // A numeric id means the same as a string one.
                    name: "Sport".to_owned(),
                    remote_id: "2".to_owned(),
                },
            ]
        );
    }
}
