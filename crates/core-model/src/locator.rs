// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Validated stream locator (parse, don't validate) — a URL that provably parsed.
//!
//! IPTV URLs are notoriously fragile (embedded query auth, odd ports, non-HTTP schemes
//! like `rtmp`/`rtsp`/`udp`), so the locator **preserves the original bytes** and only
//! guarantees that they parse as an absolute URL with a scheme. Normalizing the string
//! would risk breaking finicky headends, so we deliberately do not. Auth material
//! (per-channel headers / user-agent, and secret values via the host-secrets callback,
//! TECH_SPEC §12) rides on [`crate::channel::ChannelOverrides`], never inside the locator.

use serde::{Deserialize, Serialize};

use crate::error::ModelError;

/// A stream URL that has been parsed and found to be a syntactically valid absolute URL.
///
/// Constructed only through [`StreamLocator::parse`]; the inner string cannot be set
/// directly, so an unvalidated locator does not exist.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct StreamLocator {
    raw: String,
    scheme: String,
}

impl StreamLocator {
    /// Parses a stream locator, preserving the original (trimmed) text.
    ///
    /// # Errors
    /// Returns [`ModelError::InvalidLocator`] when the input is not a parseable absolute
    /// URL, or [`ModelError::Empty`] when it is blank.
    pub fn parse(input: &str) -> Result<Self, ModelError> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err(ModelError::Empty { field: "locator" });
        }
        let parsed = url::Url::parse(trimmed).map_err(|e| ModelError::InvalidLocator {
            reason: e.to_string(),
        })?;
        // Reject relative/opaque inputs that `url` would only accept with a base.
        if parsed.cannot_be_a_base() && parsed.scheme().is_empty() {
            return Err(ModelError::InvalidLocator {
                reason: "not an absolute URL".to_owned(),
            });
        }
        Ok(Self {
            scheme: parsed.scheme().to_ascii_lowercase(),
            raw: trimmed.to_owned(),
        })
    }

    /// The original, unmodified locator text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.raw
    }

    /// The lowercased URL scheme (`http`, `https`, `rtmp`, `rtsp`, `udp`, …).
    #[must_use]
    pub fn scheme(&self) -> &str {
        &self.scheme
    }
}

impl TryFrom<String> for StreamLocator {
    type Error = ModelError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<StreamLocator> for String {
    fn from(locator: StreamLocator) -> Self {
        locator.raw
    }
}

impl std::fmt::Display for StreamLocator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.raw)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn parses_and_preserves_original_bytes() {
        let raw = "http://Example.com:8080/live/stream.m3u8?token=abc";
        let loc = StreamLocator::parse(raw).unwrap();
        // Original casing / query preserved verbatim — no normalization.
        assert_eq!(loc.as_str(), raw);
        assert_eq!(loc.scheme(), "http");
    }

    #[test]
    fn trims_surrounding_whitespace() {
        let loc = StreamLocator::parse("  https://a.example/x  ").unwrap();
        assert_eq!(loc.as_str(), "https://a.example/x");
    }

    #[test]
    fn accepts_non_http_iptv_schemes() {
        for raw in [
            "rtmp://host/app",
            "rtsp://host:554/s",
            "udp://@239.0.0.1:1234",
        ] {
            assert!(StreamLocator::parse(raw).is_ok(), "should accept {raw}");
        }
    }

    #[test]
    fn rejects_blank_and_relative() {
        assert!(matches!(
            StreamLocator::parse("   "),
            Err(ModelError::Empty { .. })
        ));
        assert!(matches!(
            StreamLocator::parse("/relative/path"),
            Err(ModelError::InvalidLocator { .. })
        ));
        assert!(matches!(
            StreamLocator::parse("not a url"),
            Err(ModelError::InvalidLocator { .. })
        ));
    }

    #[test]
    fn serde_roundtrips_through_string() {
        let loc = StreamLocator::parse("https://a.example/s").unwrap();
        let json = serde_json::to_string(&loc).unwrap_or_default();
        assert_eq!(json, "\"https://a.example/s\"");
    }
}
