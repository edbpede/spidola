// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The owned Tokio multi-thread runtime (invisible to the shells) and the blocking adapter
//! (TECH_SPEC §4.6, rust-dev-pro.md async discipline).
//!
//! `core-api` creates exactly one multi-thread runtime at initialization and owns it for the
//! process lifetime. The UniFFI-exported `async fn`s are thin: they immediately hop onto this
//! runtime — network work via [`CoreRuntime::spawn`] (so reqwest's reactor and timers are
//! ours), blocking `core-db` work via [`CoreRuntime::run_blocking`] (the runtime's
//! blocking-thread pool) — and only `.await` the resulting handle. That is how "blocking work
//! never sits on an async worker thread" is enforced structurally: every `core-db` entry point
//! is blocking and is only ever reached through [`CoreRuntime::run_blocking`].

use std::future::Future;

use tokio::runtime::{Handle, Runtime};
use tokio::task::JoinHandle;
use tracing::error;

use crate::error::ApiError;
use crate::logging::targets;

/// The process-wide async runtime owned by the core.
pub struct CoreRuntime {
    inner: Runtime,
}

impl CoreRuntime {
    /// Builds the multi-thread runtime with the IO and time drivers enabled.
    ///
    /// # Errors
    /// Returns [`ApiError::Internal`] if the OS refuses the runtime's worker threads; the
    /// underlying `io::Error` is logged, never surfaced to the UI.
    pub fn new() -> Result<Self, ApiError> {
        let inner = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("spidola-core")
            .build()
            .map_err(|e| {
                error!(target: targets::IMPORT, cause = %e, "failed to build the core runtime");
                ApiError::Internal
            })?;
        Ok(Self { inner })
    }

    /// A cloneable handle to the runtime, for spawning from other owned contexts.
    #[must_use]
    pub fn handle(&self) -> Handle {
        self.inner.handle().clone()
    }

    /// Spawns an async task (e.g. the streaming import) onto the runtime.
    pub fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.inner.spawn(future)
    }

    /// The blocking adapter: runs a blocking closure (a `core-db` call) on the runtime's
    /// blocking-thread pool and yields its result. The returned future can be awaited from the
    /// UniFFI-driven export future without holding any async worker thread.
    ///
    /// # Errors
    /// Propagates the closure's `Result`, or returns [`ApiError::Internal`] if the blocking
    /// task panics or is cancelled (a panic is logged; it must never cross the FFI, TECH_SPEC
    /// §5).
    pub async fn run_blocking<T, F>(&self, work: F) -> Result<T, ApiError>
    where
        F: FnOnce() -> Result<T, ApiError> + Send + 'static,
        T: Send + 'static,
    {
        match self.inner.spawn_blocking(work).await {
            Ok(result) => result,
            Err(join) => {
                error!(target: targets::DB, cause = %join, "blocking core task failed to join");
                Err(ApiError::Internal)
            }
        }
    }
}
