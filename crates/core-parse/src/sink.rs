// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The batch sink trait shared by both parsers, plus the raw [`ParsedChannel`] they emit
//! (TECH_SPEC §4.2).
//!
//! Parsers push channels in batches through a caller-provided [`ChannelSink`] and never
//! materialize the whole playlist: peak memory is bounded to one batch. A [`ParsedChannel`]
//! is *raw* — strings straight from the playlist, unknown attributes preserved — and is
//! mapped into a domain `Channel` (identity derived, locator validated) by the importer,
//! keeping this crate free of `core-model` and any validation policy.

use std::collections::BTreeMap;

/// A channel as parsed from a playlist, before domain validation.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedChannel {
    /// Display name (from `#EXTINF`'s trailing text, or the URL as a fallback).
    pub name: String,
    /// Raw stream URL, validated downstream by the importer.
    pub url: String,
    /// Duration in seconds from `#EXTINF` (`-1` for live), if parseable.
    pub duration_secs: Option<f64>,
    /// **All** `#EXTINF` attributes, known and unknown, preserved verbatim.
    pub attributes: BTreeMap<String, String>,
    /// Per-channel user-agent from `#EXTVLCOPT:http-user-agent`, if present.
    pub user_agent: Option<String>,
    /// Per-channel HTTP headers derived from `#EXTVLCOPT` (e.g. `Referer`).
    pub headers: Vec<(String, String)>,
}

impl ParsedChannel {
    /// The `tvg-id` attribute, if present — the most stable identity key.
    #[must_use]
    pub fn tvg_id(&self) -> Option<&str> {
        self.attributes.get("tvg-id").map(String::as_str)
    }

    /// The `tvg-logo` attribute (logo URL), if present.
    #[must_use]
    pub fn logo(&self) -> Option<&str> {
        self.attributes.get("tvg-logo").map(String::as_str)
    }

    /// The group / category label (`group-title`), if present.
    #[must_use]
    pub fn group(&self) -> Option<&str> {
        self.attributes.get("group-title").map(String::as_str)
    }
}

/// A destination for batches of parsed channels.
///
/// The parser hands the sink ownership of each full batch (and a final partial one), then
/// starts a fresh batch — so the sink can move the channels straight into a DB transaction
/// while the parser's live memory stays bounded to `batch_size`.
pub trait ChannelSink {
    /// The sink's own failure type (e.g. a storage error).
    type Error: std::error::Error + 'static;

    /// Consumes one batch of parsed channels.
    ///
    /// # Errors
    /// Returns the sink's error to abort parsing.
    fn accept(&mut self, batch: Vec<ParsedChannel>) -> Result<(), Self::Error>;
}
