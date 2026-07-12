// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Skipped-entry accounting (TECH_SPEC §4.2), surfaced in import results so the UI can say
//! "N channels, M skipped" (PRD §6.1) without exposing parser jargon.
//!
//! The ledger's invariant — `total_seen == emitted + skipped` — is a property-tested law:
//! every entry the parser considers is either emitted or accounted as a skip with a reason.

use std::collections::BTreeMap;

/// Why an entry was skipped rather than emitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SkipReason {
    /// A `#EXTINF` had no following URL (superseded by another `#EXTINF` or ended the file).
    MissingUrl,
    /// A non-directive, non-URL line that could not be a stream.
    StrayLine,
    /// A single line exceeded the maximum length and was discarded (bounded memory).
    OversizedLine,
}

/// The running tally of a parse pass.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Diagnostics {
    total_seen: u64,
    emitted: u64,
    skipped: u64,
    reasons: BTreeMap<SkipReason, u64>,
}

impl Diagnostics {
    /// Records an emitted channel.
    pub(crate) fn record_emitted(&mut self) {
        self.total_seen += 1;
        self.emitted += 1;
    }

    /// Records a skipped entry with its reason.
    pub(crate) fn record_skip(&mut self, reason: SkipReason) {
        self.total_seen += 1;
        self.skipped += 1;
        *self.reasons.entry(reason).or_insert(0) += 1;
    }

    /// Total entries considered (`emitted + skipped`).
    #[must_use]
    pub fn total_seen(&self) -> u64 {
        self.total_seen
    }

    /// Channels emitted to the sink.
    #[must_use]
    pub fn emitted(&self) -> u64 {
        self.emitted
    }

    /// Entries skipped.
    #[must_use]
    pub fn skipped(&self) -> u64 {
        self.skipped
    }

    /// How many entries were skipped for `reason`.
    #[must_use]
    pub fn skips_for(&self, reason: SkipReason) -> u64 {
        self.reasons.get(&reason).copied().unwrap_or(0)
    }

    /// Whether the accounting invariant holds (`total_seen == emitted + skipped`).
    #[must_use]
    pub fn is_balanced(&self) -> bool {
        self.total_seen == self.emitted + self.skipped
    }
}
