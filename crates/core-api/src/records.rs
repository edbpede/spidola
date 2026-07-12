// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The flat, owned records the FFI boundary speaks in (TECH_SPEC §5).
//!
//! `core-model` owns the domain aggregates; those carry newtypes (`SourceId`,
//! `StreamLocator`, `ChannelIdentity`), borrowed views, and validation invariants that must
//! not leak across the boundary. This module holds their **flattened** mirror: plain owned
//! records and enums built only from UniFFI-native types (integers, strings, options, lists,
//! and nested records/enums), with `From` conversions in one audited place. Keeping the
//! boundary shape separate from the domain is deliberate, not duplication — the two evolve
//! for different reasons (domain correctness vs. a stable, versioned wire contract, §13).
//!
//! Identity values (`ChannelIdentity`, a `u64`) cross as the bit-equivalent `i64` SQLite
//! stores, so a shell can round-trip a channel's identity straight back into the favorites
//! API without a lossy `u64`/`ULong` hop on Kotlin.

use core_model::channel::{
    Channel as DomainChannel, ChannelOverrides as DomainOverrides, MediaKind as DomainMediaKind,
};
use core_model::favorite::Favorite as DomainFavorite;
use core_model::ids::{CategoryId, ChannelIdentity};
use core_model::source::{
    Source as DomainSource, SourceCommon as DomainCommon, SourceKind as DomainSourceKind,
};

/// What a channel plays. The shell reserves an "unknown future variant" arm (TECH_SPEC §5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MediaKind {
    /// A live channel.
    Live,
    /// A single movie / VOD title.
    Movie,
    /// One episode of a series.
    SeriesEpisode,
}

impl From<DomainMediaKind> for MediaKind {
    fn from(kind: DomainMediaKind) -> Self {
        match kind {
            DomainMediaKind::Live => Self::Live,
            DomainMediaKind::Movie => Self::Movie,
            DomainMediaKind::SeriesEpisode => Self::SeriesEpisode,
        }
    }
}

impl From<MediaKind> for DomainMediaKind {
    fn from(kind: MediaKind) -> Self {
        match kind {
            MediaKind::Live => Self::Live,
            MediaKind::Movie => Self::Movie,
            MediaKind::SeriesEpisode => Self::SeriesEpisode,
        }
    }
}

/// The storage discriminant for a [`Source`], independent of the variant payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum SourceKind {
    /// [`Source::M3uUrl`].
    M3uUrl,
    /// [`Source::M3uFile`].
    M3uFile,
    /// [`Source::Xtream`].
    Xtream,
}

impl From<DomainSourceKind> for SourceKind {
    fn from(kind: DomainSourceKind) -> Self {
        match kind {
            DomainSourceKind::M3uUrl => Self::M3uUrl,
            DomainSourceKind::M3uFile => Self::M3uFile,
            DomainSourceKind::Xtream => Self::Xtream,
        }
    }
}

/// Settings every source kind shares.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SourceCommon {
    /// User-facing display name.
    pub name: String,
    /// Whether the source participates in browse/search/refresh (PRD §6.1).
    pub enabled: bool,
    /// Optional automatic refresh interval in seconds; `None` means manual-only.
    pub auto_refresh_secs: Option<u32>,
}

impl From<DomainCommon> for SourceCommon {
    fn from(common: DomainCommon) -> Self {
        Self {
            name: common.name,
            enabled: common.enabled,
            auto_refresh_secs: common.auto_refresh_secs,
        }
    }
}

/// A configured content source. Mirrors `core_model::source::Source` as a flat enum; the
/// Xtream password is never here — only its opaque host-secrets key (TECH_SPEC §12).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum Source {
    /// An M3U/M3U8 playlist fetched from a URL.
    M3uUrl {
        /// Persisted identity.
        id: i64,
        /// Common per-source settings.
        common: SourceCommon,
        /// The playlist URL.
        url: String,
        /// Optional per-source user-agent override for fetching.
        user_agent: Option<String>,
        /// Opt-in "accept invalid TLS" escape hatch, off by default.
        accept_invalid_tls: bool,
    },
    /// An M3U/M3U8 playlist imported from a local file.
    M3uFile {
        /// Persisted identity.
        id: i64,
        /// Common per-source settings.
        common: SourceCommon,
    },
    /// An Xtream Codes account. The password is referenced, never inline.
    Xtream {
        /// Persisted identity.
        id: i64,
        /// Common per-source settings.
        common: SourceCommon,
        /// Xtream server base URL.
        server: String,
        /// Account username (not secret).
        username: String,
        /// Opaque host-secrets key naming the account password.
        secret_ref: String,
    },
}

impl From<DomainSource> for Source {
    fn from(source: DomainSource) -> Self {
        match source {
            DomainSource::M3uUrl {
                id,
                common,
                url,
                user_agent,
                accept_invalid_tls,
            } => Self::M3uUrl {
                id: id.value(),
                common: common.into(),
                url: url.to_string(),
                user_agent,
                accept_invalid_tls,
            },
            DomainSource::M3uFile { id, common } => Self::M3uFile {
                id: id.value(),
                common: common.into(),
            },
            DomainSource::Xtream {
                id,
                common,
                server,
                username,
                secret,
            } => Self::Xtream {
                id: id.value(),
                common: common.into(),
                server: server.to_string(),
                username,
                secret_ref: secret.as_str().to_owned(),
            },
        }
    }
}

/// A single per-channel HTTP header override. Token-bearing values must be sourced via the
/// host-secrets callback, never persisted (TECH_SPEC §12).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct HeaderField {
    /// Header name.
    pub name: String,
    /// Header value.
    pub value: String,
}

/// Per-channel overrides applied at fetch/playback time.
#[derive(Debug, Clone, Default, PartialEq, Eq, uniffi::Record)]
pub struct ChannelOverrides {
    /// Optional user-agent for this channel's stream.
    pub user_agent: Option<String>,
    /// Extra request headers for this channel's stream.
    pub headers: Vec<HeaderField>,
    /// Opaque preferred-engine key resolved by the shell's selection policy (TECH_SPEC §8).
    pub preferred_engine: Option<String>,
}

impl From<DomainOverrides> for ChannelOverrides {
    fn from(overrides: DomainOverrides) -> Self {
        Self {
            user_agent: overrides.user_agent,
            headers: overrides
                .headers
                .into_iter()
                .map(|(name, value)| HeaderField { name, value })
                .collect(),
            preferred_engine: overrides.preferred_engine,
        }
    }
}

/// A channel within the current catalog snapshot of a source.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Channel {
    /// Current rowid. Not stable across refresh — see `identity`.
    pub id: i64,
    /// Owning source.
    pub source_id: i64,
    /// Stable per-source identity (favorites/hidden key on this), as the stored `i64`.
    pub identity: i64,
    /// Display name.
    pub name: String,
    /// Group / category label from the playlist, if any.
    pub group_title: Option<String>,
    /// Logo URL, if any (public artwork; fetched by the shell image pipeline).
    pub logo: Option<String>,
    /// Validated stream locator (the original, unnormalized URL bytes).
    pub locator: String,
    /// What the channel plays.
    pub kind: MediaKind,
    /// Resolved category rowid, if the channel belongs to one.
    pub category_id: Option<i64>,
    /// Per-channel overrides.
    pub overrides: ChannelOverrides,
}

impl From<DomainChannel> for Channel {
    fn from(channel: DomainChannel) -> Self {
        Self {
            id: channel.id.value(),
            source_id: channel.source_id.value(),
            identity: channel.identity.to_storage(),
            name: channel.name,
            group_title: channel.group_title,
            logo: channel.logo,
            locator: channel.locator.to_string(),
            kind: channel.kind.into(),
            category_id: channel.category.map(CategoryId::value),
            overrides: channel.overrides.into(),
        }
    }
}

/// A user-marked favorite channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct Favorite {
    /// Owning source.
    pub source_id: i64,
    /// Stable identity of the favorited channel, as the stored `i64`.
    pub identity: i64,
    /// When it was favorited, Unix seconds.
    pub created_at_unix: i64,
}

impl From<DomainFavorite> for Favorite {
    fn from(favorite: DomainFavorite) -> Self {
        Self {
            source_id: favorite.source_id.value(),
            identity: favorite.identity.to_storage(),
            created_at_unix: favorite.created_at_unix,
        }
    }
}

/// A page of a source's channels (paged by contract, TECH_SPEC §4.6).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ChannelPage {
    /// The channels in this page, in playlist order.
    pub channels: Vec<Channel>,
    /// The offset this page started at.
    pub offset: u32,
    /// Total channels in the source's catalog, so the shell knows when it has them all.
    pub total: u64,
}

/// A page of search results plus whether the fuzzy fallback produced them.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SearchPage {
    /// Matching channels, most relevant first.
    pub channels: Vec<Channel>,
    /// The offset this page started at.
    pub offset: u32,
    /// `true` when these came from the trigram fallback rather than the prefix index.
    pub fuzzy: bool,
}

/// One stored setting as an opaque key/value pair. The typed settings surface and defaults
/// land in Phase 6; the boundary exposes the raw store today.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SettingEntry {
    /// Opaque setting key.
    pub key: String,
    /// Stored value.
    pub value: String,
}

/// A convenience for turning the storage `i64` a shell holds back into an identity.
#[must_use]
pub(crate) fn identity_from_storage(value: i64) -> ChannelIdentity {
    ChannelIdentity::from_storage(value)
}
