// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! `core-db`'s error taxonomy (TECH_SPEC §4.7, standing rule).
//!
//! Every fallible entry point returns [`DbError`]; source chains are preserved via
//! `#[from]`/`#[source]` so `core-api` can flatten them into the FFI taxonomy without
//! losing the underlying cause (which goes to the log stream, never the FFI error).

use thiserror::Error;

/// A persistence-layer failure.
#[derive(Debug, Error)]
pub enum DbError {
    /// Opening the database or a pooled connection failed.
    #[error("failed to open the database connection")]
    Connection(#[source] rusqlite::Error),

    /// Applying forward-only migrations failed.
    #[error("failed to apply migrations")]
    Migration(#[from] rusqlite_migration::Error),

    /// A SQL statement failed.
    #[error("database query failed")]
    Query(#[from] rusqlite::Error),

    /// (De)serializing a stored aggregate field failed.
    #[error("failed to (de)serialize a stored value")]
    Serde(#[from] serde_json::Error),

    /// A stored value violated a domain invariant on read (e.g. an unrecognized enum).
    #[error("stored data is inconsistent: {0}")]
    Integrity(String),

    /// Preparing the temp-file staging database for a writer-free refresh failed
    /// ([`crate::refresh`]). Flattens to the same storage-problem class at the FFI as any
    /// other persistence failure.
    #[error("failed to prepare the staging database")]
    Staging(#[source] std::io::Error),
}

/// Result alias for the persistence layer.
pub type DbResult<T> = Result<T, DbError>;
