// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! User-created channels and groups (PRD §6.7).

use serde::{Deserialize, Serialize};

use crate::ids::{CustomChannelId, CustomGroupId};
use crate::locator::StreamLocator;

/// A user-defined channel group.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomGroup {
    /// Persisted identity.
    pub id: CustomGroupId,
    /// Couch-facing name.
    pub name: String,
    /// Stable user-defined ordering.
    pub position: i64,
}

/// A user-defined channel. Request details are sealed before persistence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomChannel {
    /// Persisted identity.
    pub id: CustomChannelId,
    /// Optional owning group.
    pub group_id: Option<CustomGroupId>,
    /// Couch-facing name.
    pub name: String,
    /// Optional public artwork URL.
    pub logo: Option<String>,
    /// Authenticated-encrypted locator at rest.
    pub locator: StreamLocator,
    /// Authenticated-encrypted user-agent at rest.
    pub user_agent: Option<String>,
    /// Header names plus authenticated-encrypted values at rest.
    pub headers: Vec<(String, String)>,
    /// Stable order within its group.
    pub position: i64,
}
