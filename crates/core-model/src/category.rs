// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The [`Category`] type — a browse grouping within a source (TECH_SPEC §4.1).

use serde::{Deserialize, Serialize};

use crate::channel::MediaKind;
use crate::ids::{CategoryId, SourceId};

/// A category / group along the `source → type → category → channel` browse axis
/// (PRD §6.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Category {
    /// Persisted identity.
    pub id: CategoryId,
    /// Owning source.
    pub source_id: SourceId,
    /// Which content type this category groups.
    pub kind: MediaKind,
    /// Display name.
    pub name: String,
    /// The source's own id for this category (Xtream `category_id`, M3U group name), if
    /// any — used to re-associate on refresh.
    pub remote_id: Option<String>,
}
