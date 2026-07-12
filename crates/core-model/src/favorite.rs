// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The [`Favorite`] type (TECH_SPEC §4.1, PRD §6.5).
//!
//! A favorite is keyed by `(source, stable identity)` rather than a channel rowid, so it
//! survives a refresh that renumbers every channel (§4.4).

use serde::{Deserialize, Serialize};

use crate::ids::{ChannelIdentity, SourceId};

/// A user-marked favorite channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Favorite {
    /// Owning source.
    pub source_id: SourceId,
    /// Stable identity of the favorited channel.
    pub identity: ChannelIdentity,
    /// When it was favorited, Unix seconds.
    pub created_at_unix: i64,
}
