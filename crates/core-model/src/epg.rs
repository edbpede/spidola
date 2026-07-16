// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The [`EpgEntry`] type — one programme in the guide (TECH_SPEC §4.1).
//!
//! The type is defined now (Phase 1) so the domain surface is complete; EPG ingestion and
//! the rolling-window store land in Phase 8 (`core-parse/xmltv`, `core-db/repo/epg`).
//! Times are Unix seconds so the domain carries no clock; parsers take an injected "now"
//! for the rolling window (§4.2).

use serde::{Deserialize, Serialize};

use crate::ids::{ChannelIdentity, EpgEntryId, SourceId};

/// A single programme entry, keyed to a channel by its stable identity so entries survive
/// a catalog refresh.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EpgEntry {
    /// Persisted identity.
    pub id: EpgEntryId,
    /// Owning source; channel identities are stable only within a source.
    pub source_id: SourceId,
    /// The channel this programme belongs to (stable identity, not rowid).
    pub channel: ChannelIdentity,
    /// Programme title.
    pub title: String,
    /// Optional long description.
    pub description: Option<String>,
    /// Start time, Unix seconds.
    pub start_unix: i64,
    /// End time, Unix seconds.
    pub end_unix: i64,
}

impl EpgEntry {
    /// Whether `now` (Unix seconds) falls within `[start, end)`.
    #[must_use]
    pub fn is_current(&self, now_unix: i64) -> bool {
        (self.start_unix..self.end_unix).contains(&now_unix)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn is_current_uses_injected_now() {
        let entry = EpgEntry {
            id: EpgEntryId::new(1),
            source_id: SourceId::new(2),
            channel: ChannelIdentity::from_raw(9),
            title: "News".to_owned(),
            description: None,
            start_unix: 100,
            end_unix: 200,
        };
        assert!(!entry.is_current(99));
        assert!(entry.is_current(100));
        assert!(entry.is_current(199));
        assert!(!entry.is_current(200));
    }
}
