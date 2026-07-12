// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The [`PlaybackHistoryEntry`] type (TECH_SPEC §4.1, PRD §6.5).
//!
//! History snapshots the name and locator at play time and keys the channel by its stable
//! identity, so a "recently watched" entry keeps working across refreshes and remains
//! replayable even if the channel later disappears from the catalog.

use serde::{Deserialize, Serialize};

use crate::ids::{ChannelIdentity, HistoryId, SourceId};
use crate::locator::StreamLocator;

/// One "recently watched" record. Maintained locally with a purge toggle and off switch
/// (PRD §6.5); never leaves the device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaybackHistoryEntry {
    /// Persisted identity.
    pub id: HistoryId,
    /// Owning source.
    pub source_id: SourceId,
    /// Stable identity of the played channel.
    pub identity: ChannelIdentity,
    /// Channel name as it was at play time.
    pub name: String,
    /// Locator as it was at play time (for replay).
    pub locator: StreamLocator,
    /// When it was played, Unix seconds.
    pub played_at_unix: i64,
    /// Resume position in seconds, if the stream is seekable and progress was recorded.
    pub position_secs: Option<u32>,
}
