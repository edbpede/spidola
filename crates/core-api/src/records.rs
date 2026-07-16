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

use std::sync::Arc;

use core_model::channel::{
    Channel as DomainChannel, ChannelOverrides as DomainOverrides, MediaKind as DomainMediaKind,
};
use core_model::favorite::Favorite as DomainFavorite;
use core_model::history::PlaybackHistoryEntry as DomainHistory;
use core_model::ids::{CategoryId, ChannelIdentity};
use core_model::source::{
    Source as DomainSource, SourceCommon as DomainCommon, SourceKind as DomainSourceKind,
};
use core_model::{
    CustomChannel as DomainCustomChannel, CustomGroup as DomainCustomGroup,
    EpgEntry as DomainEpgEntry,
};
use zeroize::Zeroize;

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

/// A configured content source. Mirrors `core_model::source::Source` as a flat enum. Secret
/// values and their opaque keys stay inside the core; the shell receives only display/settings
/// metadata (TECH_SPEC §12).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum Source {
    /// An M3U/M3U8 playlist fetched from a URL.
    M3uUrl {
        /// Persisted identity.
        id: i64,
        /// Common per-source settings.
        common: SourceCommon,
        /// Whether a secure per-source user-agent override is configured.
        has_user_agent: bool,
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
                has_user_agent,
                accept_invalid_tls,
                ..
            } => Self::M3uUrl {
                id: id.value(),
                common: common.into(),
                has_user_agent,
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
                server: server.as_str().to_owned(),
                username,
                secret_ref: secret.as_str().to_owned(),
            },
        }
    }
}

/// A single per-channel HTTP header override. Catalog records carry authenticated envelopes;
/// [`ResolvedHeader`] exposes the plaintext value only at play time (TECH_SPEC §12).
#[derive(Clone, PartialEq, Eq, uniffi::Record)]
pub struct HeaderField {
    /// Header name.
    pub name: String,
    /// Header value.
    pub value: String,
}

impl std::fmt::Debug for HeaderField {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HeaderField")
            .field("name", &self.name)
            .field("value", &"[REDACTED]")
            .finish()
    }
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

/// One plaintext HTTP header exposed only as an opaque play-time object. Generated Swift/Kotlin
/// representations therefore cannot print its value by reflecting a record's stored fields.
#[derive(uniffi::Object)]
pub struct ResolvedHeader {
    name: String,
    value: String,
}

impl ResolvedHeader {
    pub(crate) fn new(name: String, value: String) -> Arc<Self> {
        Arc::new(Self { name, value })
    }

    pub(crate) fn pair(&self) -> (&str, &str) {
        (&self.name, &self.value)
    }
}

impl std::fmt::Debug for ResolvedHeader {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ResolvedHeader")
            .field("name", &self.name)
            .field("value", &"[REDACTED]")
            .finish()
    }
}

impl Drop for ResolvedHeader {
    fn drop(&mut self) {
        self.name.zeroize();
        self.value.zeroize();
    }
}

#[uniffi::export]
impl ResolvedHeader {
    /// Constructs an opaque header for a create/edit request.
    #[uniffi::constructor]
    #[must_use]
    pub fn from_parts(name: String, value: String) -> Arc<Self> {
        Self::new(name, value)
    }

    /// Header name. Returned only when the shell constructs the engine request.
    #[must_use]
    pub fn name(&self) -> String {
        self.name.clone()
    }

    /// Plaintext header value. Never persist or log the returned string.
    #[must_use]
    pub fn value(&self) -> String {
        self.value.clone()
    }
}

/// Everything the shell needs to construct an engine request after the core has opened the
/// catalog's authenticated envelopes. This opaque object is ephemeral: never persist or log it.
#[derive(uniffi::Object)]
pub struct ResolvedStream {
    /// Playable locator with any source credentials restored.
    locator: String,
    /// Plaintext per-channel user-agent, if present.
    user_agent: Option<String>,
    /// Plaintext per-channel HTTP headers.
    headers: Vec<Arc<ResolvedHeader>>,
}

impl ResolvedStream {
    pub(crate) fn new(
        locator: String,
        user_agent: Option<String>,
        headers: Vec<Arc<ResolvedHeader>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            locator,
            user_agent,
            headers,
        })
    }
}

impl std::fmt::Debug for ResolvedStream {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ResolvedStream")
            .field("locator", &"[REDACTED]")
            .field(
                "user_agent",
                &self.user_agent.as_ref().map(|_| "[REDACTED]"),
            )
            .field("headers", &self.headers)
            .finish()
    }
}

impl Drop for ResolvedStream {
    fn drop(&mut self) {
        self.locator.zeroize();
        self.user_agent.zeroize();
    }
}

#[uniffi::export]
impl ResolvedStream {
    /// Playable locator with source credentials restored. Never persist or log it.
    #[must_use]
    pub fn locator(&self) -> String {
        self.locator.clone()
    }

    /// Plaintext per-channel user-agent, if present. Never persist or log it.
    #[must_use]
    pub fn user_agent(&self) -> Option<String> {
        self.user_agent.clone()
    }

    /// Opaque plaintext header handles for immediate engine construction.
    #[must_use]
    pub fn headers(&self) -> Vec<Arc<ResolvedHeader>> {
        self.headers.clone()
    }
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
            locator: channel.locator.as_str().to_owned(),
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
    /// Explicit user-defined lineup position.
    pub position: i64,
}

impl From<DomainFavorite> for Favorite {
    fn from(favorite: DomainFavorite) -> Self {
        Self {
            source_id: favorite.source_id.value(),
            identity: favorite.identity.to_storage(),
            created_at_unix: favorite.created_at_unix,
            position: favorite.position,
        }
    }
}

/// One programme in the rolling EPG window.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct EpgProgramme {
    pub id: i64,
    pub source_id: i64,
    pub channel_identity: i64,
    pub title: String,
    pub description: Option<String>,
    pub start_unix: i64,
    pub end_unix: i64,
}

impl From<DomainEpgEntry> for EpgProgramme {
    fn from(entry: DomainEpgEntry) -> Self {
        Self {
            id: entry.id.value(),
            source_id: entry.source_id.value(),
            channel_identity: entry.channel.to_storage(),
            title: entry.title,
            description: entry.description,
            start_unix: entry.start_unix,
            end_unix: entry.end_unix,
        }
    }
}

/// Current and next programme for one channel.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct NowNext {
    pub current: Option<EpgProgramme>,
    pub next: Option<EpgProgramme>,
}

/// A bounded EPG page.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct EpgPage {
    pub programmes: Vec<EpgProgramme>,
    pub offset: u32,
}

/// A user-created channel group.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct CustomGroup {
    pub id: i64,
    pub name: String,
    pub position: i64,
}

impl From<DomainCustomGroup> for CustomGroup {
    fn from(group: DomainCustomGroup) -> Self {
        Self {
            id: group.id.value(),
            name: group.name,
            position: group.position,
        }
    }
}

/// Secret-safe summary of a user-created channel.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct CustomChannelSummary {
    pub id: i64,
    pub group_id: Option<i64>,
    pub name: String,
    pub logo: Option<String>,
    pub has_user_agent: bool,
    pub header_count: u32,
    pub position: i64,
}

impl From<DomainCustomChannel> for CustomChannelSummary {
    fn from(channel: DomainCustomChannel) -> Self {
        Self {
            id: channel.id.value(),
            group_id: channel.group_id.map(core_model::CustomGroupId::value),
            name: channel.name,
            logo: channel.logo,
            has_user_agent: channel.user_agent.is_some(),
            header_count: u32::try_from(channel.headers.len()).unwrap_or(u32::MAX),
            position: channel.position,
        }
    }
}

/// Opaque create/edit payload so locator and request details cannot appear in generated
/// record diagnostics.
#[derive(uniffi::Object)]
pub struct CustomChannelDraft {
    pub(crate) group_id: Option<i64>,
    pub(crate) name: String,
    pub(crate) logo: Option<String>,
    pub(crate) locator: String,
    pub(crate) user_agent: Option<String>,
    pub(crate) headers: Vec<Arc<ResolvedHeader>>,
}

impl Drop for CustomChannelDraft {
    fn drop(&mut self) {
        self.locator.zeroize();
        self.user_agent.zeroize();
    }
}

impl std::fmt::Debug for CustomChannelDraft {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CustomChannelDraft")
            .field("group_id", &self.group_id)
            .field("name", &self.name)
            .field("logo", &self.logo)
            .field("locator", &"[REDACTED]")
            .field(
                "user_agent",
                &self.user_agent.as_ref().map(|_| "[REDACTED]"),
            )
            .field("headers", &self.headers)
            .finish()
    }
}

#[uniffi::export]
impl CustomChannelDraft {
    #[uniffi::constructor]
    #[must_use]
    pub fn new(
        group_id: Option<i64>,
        name: String,
        logo: Option<String>,
        locator: String,
        user_agent: Option<String>,
        headers: Vec<Arc<ResolvedHeader>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            group_id,
            name,
            logo,
            locator,
            user_agent,
            headers,
        })
    }
}

/// Conflict behavior for portable custom-channel imports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum CustomImportMode {
    /// Keep existing channels and add imported rows after them.
    Merge,
    /// Replace the complete custom-channel catalog atomically.
    Replace,
}

/// Opaque portable export. The contents may include user-supplied credentials and are exposed
/// only through the explicit getter used by the platform document exporter.
#[derive(uniffi::Object)]
pub struct CustomChannelExport {
    contents: String,
}

impl CustomChannelExport {
    pub(crate) fn new(contents: String) -> Arc<Self> {
        Arc::new(Self { contents })
    }
}

impl std::fmt::Debug for CustomChannelExport {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("CustomChannelExport([REDACTED])")
    }
}

impl Drop for CustomChannelExport {
    fn drop(&mut self) {
        self.contents.zeroize();
    }
}

#[uniffi::export]
impl CustomChannelExport {
    /// Returns the versioned JSON for immediate writing to a user-chosen file. Never log it.
    #[must_use]
    pub fn contents(&self) -> String {
        self.contents.clone()
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

/// One distinct group within a source's catalog — a "category" in the browse drill-down
/// (source → type → category → channel). `None` title is the "ungrouped" bucket.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BrowseGroup {
    /// The playlist group label; `None` is the ungrouped bucket.
    pub title: Option<String>,
    /// Visible (non-hidden) channels in this group.
    pub channel_count: u64,
}

/// A page of a source's browse groups (paged by contract, §4.6).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BrowseGroupPage {
    /// The groups in this page.
    pub groups: Vec<BrowseGroup>,
    /// The offset this page started at.
    pub offset: u32,
    /// Total distinct groups for the source and media kind.
    pub total: u64,
}

/// A "recently watched" entry (PRD §6.5). Snapshots the name and locator at play time and
/// keys the channel by stable identity, so it stays replayable across refreshes even if the
/// channel later leaves the catalog. Never leaves the device.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Recent {
    /// Owning source.
    pub source_id: i64,
    /// Stable identity of the played channel, as the stored `i64`.
    pub identity: i64,
    /// Channel name as it was at play time.
    pub name: String,
    /// Locator as it was at play time (for replay).
    pub locator: String,
    /// When it was played, Unix seconds.
    pub played_at_unix: i64,
    /// Resume position in seconds, if recorded.
    pub position_secs: Option<u32>,
}

impl From<DomainHistory> for Recent {
    fn from(entry: DomainHistory) -> Self {
        Self {
            source_id: entry.source_id.value(),
            identity: entry.identity.to_storage(),
            name: entry.name,
            locator: entry.locator.as_str().to_owned(),
            played_at_unix: entry.played_at_unix,
            position_secs: entry.position_secs,
        }
    }
}

/// A convenience for turning the storage `i64` a shell holds back into an identity.
#[must_use]
pub(crate) fn identity_from_storage(value: i64) -> ChannelIdentity {
    ChannelIdentity::from_storage(value)
}
