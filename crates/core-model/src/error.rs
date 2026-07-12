// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! `core-model`'s error taxonomy (TECH_SPEC §4.7, standing rule).
//!
//! Domain types use "parse, don't validate" constructors; when construction is refused it
//! returns a precise, matchable [`ModelError`] rather than panicking. Higher layers map
//! these into their own taxonomies (`core-db`, `core-api`) with source chains preserved.

use thiserror::Error;

/// A domain value could not be constructed because it would be illegal.
#[derive(Debug, Error)]
pub enum ModelError {
    /// A stream locator string was not a parseable absolute URL.
    #[error("invalid stream locator: {reason}")]
    InvalidLocator {
        /// Human-readable reason (never contains credential material).
        reason: String,
    },

    /// A required field was empty or whitespace-only.
    #[error("`{field}` must not be empty")]
    Empty {
        /// The offending field name.
        field: &'static str,
    },
}
