// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! `core-search`'s error taxonomy (TECH_SPEC §4.7, standing rule).

use thiserror::Error;

/// A search-layer failure.
#[derive(Debug, Error)]
pub enum SearchError {
    /// The underlying SQL/FTS query failed.
    #[error("search query failed")]
    Query(#[from] rusqlite::Error),

    /// A stored value violated a domain invariant on read.
    #[error("stored data is inconsistent: {0}")]
    Integrity(String),
}

/// Result alias for the search layer.
pub type SearchResult<T> = Result<T, SearchError>;
