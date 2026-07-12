// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The host-secrets callback interface (Keychain / Keystore-backed, TECH_SPEC §12).
//!
//! Secrets (Xtream passwords, token-bearing header values) never persist in SQLite and never
//! reach the log stream. The core stores only an opaque [`SecretRef`](core_model::ids::SecretRef)
//! key in the DB; the actual value lives in the host's secure store. When the core needs a
//! secret — to authenticate an Xtream request or embed a stream credential (Phase 6) — it
//! asks the shell through this callback, keyed by that opaque string.
//!
//! **Threading contract:** the core invokes these methods from its own worker/blocking
//! threads — they may run on *any* thread and must be safe to call concurrently. The shell's
//! implementation talks to Keychain / Keystore, which are themselves thread-safe.

use crate::error::ApiError;

/// A store the host implements over its platform secure storage.
///
/// Foreign-implemented only (the core never provides an implementation across the boundary),
/// so this is a UniFFI callback interface. Every method may be called from any core thread.
#[uniffi::export(callback_interface)]
pub trait SecretStore: Send + Sync {
    /// Retrieves the secret value stored under `key`, or `None` if there is none.
    ///
    /// # Errors
    /// Returns [`ApiError`] if the secure store is unavailable.
    fn get(&self, key: String) -> Result<Option<String>, ApiError>;

    /// Stores `value` under `key`, replacing any existing value.
    ///
    /// # Errors
    /// Returns [`ApiError`] if the secure store rejects the write.
    fn set(&self, key: String, value: String) -> Result<(), ApiError>;

    /// Deletes the secret under `key` (idempotent).
    ///
    /// # Errors
    /// Returns [`ApiError`] if the secure store rejects the delete.
    fn delete(&self, key: String) -> Result<(), ApiError>;
}
