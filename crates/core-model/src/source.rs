// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The [`Source`] enum over its three kinds: m3u-url, m3u-file, and xtream.
//!
//! Each variant carries **only** the fields that kind possesses, so illegal combinations
//! (an Xtream password on a file source, a server URL on an M3U file) cannot be
//! constructed (TECH_SPEC §4.1). Secrets never live here: the Xtream variant holds a
//! [`SecretRef`] (opaque host-secrets key), never the password itself (§12).

use serde::{Deserialize, Serialize};

use crate::ids::{SecretRef, SourceId};
use crate::locator::StreamLocator;

/// A configured content source. The discriminant maps 1:1 to [`SourceKind`] for storage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Source {
    /// An M3U/M3U8 playlist fetched from a URL.
    M3uUrl {
        /// Persisted identity.
        id: SourceId,
        /// Common per-source settings.
        common: SourceCommon,
        /// The playlist URL.
        url: StreamLocator,
        /// Optional per-source user-agent override for fetching.
        user_agent: Option<String>,
        /// Opt-in "accept invalid TLS" escape hatch, off by default (`core-fetch::tls`).
        accept_invalid_tls: bool,
    },
    /// An M3U/M3U8 playlist imported from a local file (document picker / SAF / paste).
    M3uFile {
        /// Persisted identity.
        id: SourceId,
        /// Common per-source settings.
        common: SourceCommon,
    },
    /// An Xtream Codes account. The password is referenced, never stored inline.
    Xtream {
        /// Persisted identity.
        id: SourceId,
        /// Common per-source settings.
        common: SourceCommon,
        /// Xtream server base URL.
        server: StreamLocator,
        /// Account username (not secret; the password is [`Self::Xtream::secret`]).
        username: String,
        /// Opaque host-secrets key naming the account password (TECH_SPEC §12).
        secret: SecretRef,
    },
}

/// The storage discriminant for [`Source`], independent of the variant payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SourceKind {
    /// [`Source::M3uUrl`].
    M3uUrl,
    /// [`Source::M3uFile`].
    M3uFile,
    /// [`Source::Xtream`].
    Xtream,
}

/// Settings every source kind shares.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceCommon {
    /// User-facing display name.
    pub name: String,
    /// Whether the source participates in browse/search/refresh (can be disabled, not
    /// deleted, per PRD §6.1).
    pub enabled: bool,
    /// Optional automatic refresh interval in seconds; `None` means manual-only.
    pub auto_refresh_secs: Option<u32>,
}

impl Source {
    /// The persisted identity, regardless of kind.
    #[must_use]
    pub fn id(&self) -> SourceId {
        match self {
            Self::M3uUrl { id, .. } | Self::M3uFile { id, .. } | Self::Xtream { id, .. } => *id,
        }
    }

    /// The storage discriminant for this source.
    #[must_use]
    pub fn kind(&self) -> SourceKind {
        match self {
            Self::M3uUrl { .. } => SourceKind::M3uUrl,
            Self::M3uFile { .. } => SourceKind::M3uFile,
            Self::Xtream { .. } => SourceKind::Xtream,
        }
    }

    /// The settings shared by every kind.
    #[must_use]
    pub fn common(&self) -> &SourceCommon {
        match self {
            Self::M3uUrl { common, .. }
            | Self::M3uFile { common, .. }
            | Self::Xtream { common, .. } => common,
        }
    }

    /// The same source, carrying `id`.
    ///
    /// The identity is the one thing a source's definition cannot know about itself — it is
    /// minted by the insert that persists it. This is how the caller that composed a definition
    /// names the row it just became, without asking storage to describe work it did itself.
    #[must_use]
    pub fn with_id(mut self, id: SourceId) -> Self {
        match &mut self {
            Self::M3uUrl { id: identity, .. }
            | Self::M3uFile { id: identity, .. }
            | Self::Xtream { id: identity, .. } => *identity = id,
        }
        self
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    fn common() -> SourceCommon {
        SourceCommon {
            name: "Home headend".to_owned(),
            enabled: true,
            auto_refresh_secs: Some(3600),
        }
    }

    #[test]
    fn accessors_are_kind_agnostic() {
        let src = Source::M3uUrl {
            id: SourceId::new(7),
            common: common(),
            url: StreamLocator::parse("https://a.example/list.m3u").unwrap(),
            user_agent: None,
            accept_invalid_tls: false,
        };
        assert_eq!(src.id(), SourceId::new(7));
        assert_eq!(src.kind(), SourceKind::M3uUrl);
        assert_eq!(src.common().name, "Home headend");
    }

    #[test]
    fn xtream_references_secret_not_password() {
        let src = Source::Xtream {
            id: SourceId::new(1),
            common: common(),
            server: StreamLocator::parse("http://panel.example:8080").unwrap(),
            username: "alice".to_owned(),
            secret: SecretRef::new("xtream/1/password"),
        };
        // The only credential surface is an opaque key — no password field exists.
        assert_eq!(src.kind(), SourceKind::Xtream);
    }
}
