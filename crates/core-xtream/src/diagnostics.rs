// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Skipped-entry accounting for Xtream listings (TECH_SPEC §4.3).
//!
//! The same ledger `core-parse` keeps for M3U (§4.2), for the same reason: a row the
//! headend returns in an unusable state is *skipped and counted*, never escalated into a
//! failed import, so one bad title cannot cost the user their catalog. The invariant
//! `total_seen == emitted + skipped` is asserted on every mapping path — every row the
//! mapper considers is either emitted or accounted for with a reason.
//!
//! Deliberately a separate ledger from `core_parse::m3u::diagnostics`: the reasons are
//! wire-shape-specific and sharing the type would force one crate's vocabulary onto the
//! other's failures (the M3U parser cannot produce a `MissingId`, and this one cannot
//! produce an `OversizedLine`).

use std::collections::BTreeMap;

/// Why a listing row was skipped rather than emitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SkipReason {
    /// The row was well-formed JSON but not the shape the endpoint promises (a field the
    /// mapping needs held a type no tolerant deserializer could read).
    MalformedEntry,
    /// `stream_id` / `series_id` / episode `id` was absent, empty, or zero — Xtream's three
    /// spellings of "no id". Without it no stream URL can be built.
    MissingId,
    /// The row had no usable display name, leaving nothing to render in a browse list.
    MissingName,
    /// `container_extension` was present but not a plausible file extension.
    UnusableExtension,
    /// The URL built from the row did not parse as a locator. Near-unreachable — the URL is
    /// assembled by `url`, not concatenated — but it is a skip, not a panic, if it happens.
    InvalidLocator,
}

/// The running tally of a mapping pass.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Diagnostics {
    total_seen: u64,
    emitted: u64,
    skipped: u64,
    reasons: BTreeMap<SkipReason, u64>,
}

impl Diagnostics {
    /// Records an emitted row.
    pub(crate) fn record_emitted(&mut self) {
        self.total_seen += 1;
        self.emitted += 1;
    }

    /// Records a skipped row with its reason.
    pub(crate) fn record_skip(&mut self, reason: SkipReason) {
        self.total_seen += 1;
        self.skipped += 1;
        *self.reasons.entry(reason).or_insert(0) += 1;
    }

    /// Total rows considered (`emitted + skipped`).
    #[must_use]
    pub fn total_seen(&self) -> u64 {
        self.total_seen
    }

    /// Rows mapped into the catalog.
    #[must_use]
    pub fn emitted(&self) -> u64 {
        self.emitted
    }

    /// Rows skipped.
    #[must_use]
    pub fn skipped(&self) -> u64 {
        self.skipped
    }

    /// How many rows were skipped for `reason`.
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn every_row_is_either_emitted_or_accounted_for() {
        let mut diagnostics = Diagnostics::default();
        diagnostics.record_emitted();
        diagnostics.record_emitted();
        diagnostics.record_skip(SkipReason::MissingId);
        diagnostics.record_skip(SkipReason::MissingId);
        diagnostics.record_skip(SkipReason::MalformedEntry);

        assert_eq!(diagnostics.total_seen(), 5);
        assert_eq!(diagnostics.emitted(), 2);
        assert_eq!(diagnostics.skipped(), 3);
        assert!(diagnostics.is_balanced());
    }

    #[test]
    fn skips_are_attributed_per_reason() {
        let mut diagnostics = Diagnostics::default();
        diagnostics.record_skip(SkipReason::MissingName);
        assert_eq!(diagnostics.skips_for(SkipReason::MissingName), 1);
        assert_eq!(diagnostics.skips_for(SkipReason::MissingId), 0);
    }
}
