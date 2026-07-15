// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The [`Channel`] aggregate: identity, display, group, logo, locator, kind, and
//! per-channel overrides (TECH_SPEC §4.1).
//!
//! Channels also own the derivation of the **stable per-source identity hash**
//! ([`channel_identity`]) that lets favorites and hidden flags survive a refresh even
//! though rowids churn on every staging-and-swap (§4.4).

use serde::{Deserialize, Serialize};

use crate::ids::{CategoryId, ChannelId, ChannelIdentity, SourceId};
use crate::locator::StreamLocator;

/// What a channel plays. Exhaustively matched in the core; shells reserve an
/// "unknown future variant" arm across the FFI (TECH_SPEC §5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MediaKind {
    /// A live channel.
    Live,
    /// A single movie / VOD title.
    Movie,
    /// One episode of a series.
    SeriesEpisode,
}

/// Per-channel overrides applied at fetch/playback time.
///
/// `preferred_engine` is an **opaque** engine key, not a core-defined enum: engine
/// identities (MPVKit, ExoPlayer, AVPlayer, libmpv) are shell concepts, and baking them
/// into the core would invert the layering (TECH_SPEC §8). The selection policy that
/// consumes it lives in the shells (Phase 5).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelOverrides {
    /// Optional user-agent for this channel's stream.
    pub user_agent: Option<String>,
    /// Extra request headers (name, value) for this channel's stream. Token-bearing values are
    /// authenticated-encrypted at rest and opened only by the play-time resolver (§12).
    pub headers: Vec<(String, String)>,
    /// Opaque preferred-engine key resolved by the shell's selection policy.
    pub preferred_engine: Option<String>,
}

/// A channel within the current catalog snapshot of a source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Channel {
    /// Current rowid. Not stable across refresh — see [`Channel::identity`].
    pub id: ChannelId,
    /// Owning source.
    pub source_id: SourceId,
    /// Stable per-source identity used by favorites/hidden.
    pub identity: ChannelIdentity,
    /// Display name.
    pub name: String,
    /// Group / category label from the playlist, if any.
    pub group_title: Option<String>,
    /// Logo URL, if any (public artwork; fetched by the shell image pipeline).
    pub logo: Option<String>,
    /// Validated stream locator.
    pub locator: StreamLocator,
    /// What the channel plays.
    pub kind: MediaKind,
    /// Resolved category rowid, if the channel belongs to one.
    pub category: Option<CategoryId>,
    /// Per-channel overrides.
    pub overrides: ChannelOverrides,
}

/// Derives the stable identity of a channel from the most durable attribute available.
///
/// Precedence (most→least stable): the playlist `tvg-id`, else the stream URL, else the
/// display name. The chosen key is hashed with FNV-1a-64, which is deterministic across
/// platforms and runs (no `HashMap` random seed), so the same channel hashes identically on
/// every refresh — the property favorites/hidden rely on (TECH_SPEC §4.4). Not a security
/// primitive; collision resistance at 50k entries is ample for identity.
#[must_use]
pub fn channel_identity(tvg_id: Option<&str>, url: &str, name: &str) -> ChannelIdentity {
    let key = stable_key(tvg_id, url, name);
    ChannelIdentity::from_raw(fnv1a_64(key.as_bytes()))
}

/// Selects the stable key per the documented precedence, ignoring blank candidates.
fn stable_key<'a>(tvg_id: Option<&'a str>, url: &'a str, name: &'a str) -> &'a str {
    if let Some(id) = tvg_id
        && !id.trim().is_empty()
    {
        return id.trim();
    }
    let url = url.trim();
    if !url.is_empty() {
        return url;
    }
    name.trim()
}

/// FNV-1a 64-bit. Kept private to `channel` because identity is its only consumer; if a
/// second consumer appears it earns a named home of its own (doctrine §3.1).
const fn fnv1a_64(bytes: &[u8]) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = OFFSET;
    let mut i = 0;
    while i < bytes.len() {
        hash ^= bytes[i] as u64;
        hash = hash.wrapping_mul(PRIME);
        i += 1;
    }
    hash
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn identity_prefers_tvg_id_over_url_and_name() {
        let with_id = channel_identity(Some("bbc.one"), "http://a/1", "BBC One");
        let same_id_diff_url = channel_identity(Some("bbc.one"), "http://b/2", "Whatever");
        // Same tvg-id ⇒ same identity even if URL and name change on refresh.
        assert_eq!(with_id, same_id_diff_url);
    }

    #[test]
    fn identity_falls_back_to_url_then_name() {
        let by_url = channel_identity(None, "http://a/live/1", "Name A");
        let by_url_again = channel_identity(Some("   "), "http://a/live/1", "Name B");
        assert_eq!(by_url, by_url_again);

        let by_name = channel_identity(None, "", "Only Name");
        let by_name_again = channel_identity(Some(""), "   ", "Only Name");
        assert_eq!(by_name, by_name_again);
    }

    #[test]
    fn distinct_channels_get_distinct_identities() {
        let a = channel_identity(Some("a"), "http://x", "A");
        let b = channel_identity(Some("b"), "http://x", "A");
        assert_ne!(a, b);
    }

    #[test]
    fn fnv_matches_known_vector() {
        // FNV-1a-64 of the empty string is the offset basis.
        assert_eq!(fnv1a_64(b""), 0xcbf2_9ce4_8422_2325);
        // FNV-1a-64("a") is a well-known constant.
        assert_eq!(fnv1a_64(b"a"), 0xaf63_dc4c_8601_ec8c);
    }
}
