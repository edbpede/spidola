// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Progress / task-handle callback plumbing; honest cancellation at batch boundaries
//! (TECH_SPEC §4.6, §5).
//!
//! Long operations (import / refresh) follow one pattern: the service call returns a
//! [`TaskHandle`] immediately while the work runs on the core runtime; progress, completion,
//! and failure arrive on the caller's [`ImportListener`]. Cancellation is **honest** — calling
//! [`TaskHandle::cancel`] sets a flag the import checks at batch boundaries (before each staged
//! batch and between fetched chunks), so it stops within one batch and the in-flight
//! staging-and-swap transaction is dropped un-committed, leaving the prior catalog intact
//! (TECH_SPEC §4.4). Cancellation never hard-aborts a task mid-DB-write.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::error::ApiError;

/// A shared cooperative-cancellation flag threaded from a [`TaskHandle`] into the running
/// import. Cloning shares the same underlying flag.
#[derive(Clone, Default)]
pub(crate) struct CancelToken {
    flag: Arc<AtomicBool>,
}

impl CancelToken {
    /// Requests cancellation. The import observes this at its next batch boundary.
    pub(crate) fn cancel(&self) {
        self.flag.store(true, Ordering::SeqCst);
    }

    /// Whether cancellation has been requested.
    pub(crate) fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }
}

/// A handle to a running long operation. The only operation is honest cancellation; progress
/// and the terminal outcome arrive on the [`ImportListener`] the caller registered.
#[derive(uniffi::Object)]
pub struct TaskHandle {
    token: CancelToken,
}

impl TaskHandle {
    /// Wraps a cancellation token as a handle.
    pub(crate) fn new(token: CancelToken) -> Self {
        Self { token }
    }
}

#[uniffi::export]
impl TaskHandle {
    /// Requests cancellation of the operation. Idempotent and safe to call from any thread;
    /// the operation stops at its next batch boundary and reports `Cancelled`.
    pub fn cancel(&self) {
        self.token.cancel();
    }
}

/// Which phase of an import is currently running.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ImportStage {
    /// Establishing the connection to the source.
    Connecting,
    /// Streaming and parsing the playlist body.
    Downloading,
    /// Committing the staged catalog (the staging-and-swap).
    Finalizing,
}

/// A progress update pushed as an import runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct ImportProgress {
    /// The phase the import is currently in.
    pub stage: ImportStage,
    /// Channels parsed and staged so far.
    pub channels_seen: u64,
}

/// The terminal result of a completed import.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct ImportOutcome {
    /// Channels in the new catalog after the swap.
    pub inserted: u64,
    /// Staged rows coalesced away by a shared `(source, identity)` collision.
    pub duplicates_dropped: u64,
    /// Channels the parser emitted.
    pub emitted: u64,
    /// Entries the parser skipped as malformed (skip-and-count, never a hard failure).
    pub skipped: u64,
    /// Entries dropped because their stream URL was not a valid locator.
    pub invalid: u64,
}

/// The listener a shell registers to observe an import.
///
/// Foreign-implemented only, so this is a UniFFI callback interface. **Threading contract:**
/// these methods are invoked from the core's own worker/blocking threads and may arrive on
/// *any* thread; the shell must trampoline to its main actor/dispatcher itself (TECH_SPEC §5).
/// Exactly one terminal method — [`Self::on_complete`] or [`Self::on_failed`] — is called per
/// import, after zero or more [`Self::on_progress`] calls.
#[uniffi::export(callback_interface)]
pub trait ImportListener: Send + Sync {
    /// A progress update. May be called many times, always before the terminal call.
    fn on_progress(&self, progress: ImportProgress);
    /// The import finished successfully.
    fn on_complete(&self, outcome: ImportOutcome);
    /// The import failed or was cancelled ([`ApiError::Cancelled`]).
    fn on_failed(&self, error: ApiError);
}
