// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! `core-parse`'s error taxonomy (TECH_SPEC §4.7, standing rule).
//!
//! The parsers are **tolerant**: a malformed entry is skipped and counted in the
//! diagnostics ledger, never turned into an error, so a bad line can't fail an import.
//! The only failure that propagates is the sink's own — hence the single [`ParseError`]
//! variant, generic over the sink error with its source chain preserved.

use thiserror::Error;

/// A parse-pipeline failure. Because parsing itself is skip-and-count tolerant, the only
/// way this arises is the downstream sink rejecting a batch.
#[derive(Debug, Error)]
pub enum ParseError<E: std::error::Error + 'static> {
    /// The sink failed to accept a batch.
    #[error("the channel sink rejected a batch")]
    Sink(#[source] E),
}
