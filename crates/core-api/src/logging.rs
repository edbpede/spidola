// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The `tracing` → host sink bridge and ring-buffer export (TECH_SPEC §4.8).
//!
//! One pipeline, three consumers: `tracing` spans/events in the core, forwarded to each
//! shell's native sink (`OSLog` / logcat, wired in Phase 3) and captured in a bounded
//! **ring buffer** for the diagnostics log-export. The runtime log level is a reloadable
//! `EnvFilter`, so disabled levels cost nothing (PRD diagnostics screen). Secrets never
//! reach the buffer *by construction*: the secret types' `Debug` is redacted, proven by the
//! `secrets_are_redacted_in_the_ring` test, and backed by a CI grep against formatting
//! exposed secrets in log macros.

use std::collections::VecDeque;
use std::fmt::Write as _;
use std::sync::{Arc, Mutex, PoisonError};

use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber};
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::prelude::*;
use tracing_subscriber::{EnvFilter, Registry, reload};

/// Default number of recent log lines retained for export.
pub const DEFAULT_RING_CAPACITY: usize = 2_000;

/// Per-subsystem log targets (the target-per-crate convention). Shells map one native
/// logging category to each (TECH_SPEC §4.8).
pub mod targets {
    /// Source import / refresh pipeline.
    pub const IMPORT: &str = "spidola::import";
    /// Persistence.
    pub const DB: &str = "spidola::db";
    /// HTTP fetching.
    pub const FETCH: &str = "spidola::fetch";
    /// Parsing.
    pub const PARSE: &str = "spidola::parse";
    /// Search.
    pub const SEARCH: &str = "spidola::search";
}

/// A bounded, shareable buffer of the most recent formatted log lines.
#[derive(Clone)]
pub struct RingBuffer {
    inner: Arc<Mutex<VecDeque<String>>>,
    capacity: usize,
}

impl RingBuffer {
    /// A ring holding at most `capacity` lines (`>= 1`).
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        let capacity = capacity.max(1);
        Self {
            inner: Arc::new(Mutex::new(VecDeque::with_capacity(capacity.min(4096)))),
            capacity,
        }
    }

    fn push(&self, line: String) {
        let mut guard = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        if guard.len() == self.capacity {
            guard.pop_front();
        }
        guard.push_back(line);
    }

    /// Snapshots the buffer, oldest line first — the diagnostics log-export payload.
    #[must_use]
    pub fn export(&self) -> Vec<String> {
        let guard = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        guard.iter().cloned().collect()
    }

    /// Number of lines currently buffered.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }

    /// Whether the buffer is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// A `tracing` layer that appends each event to a [`RingBuffer`].
pub struct RingLayer {
    buffer: RingBuffer,
}

impl RingLayer {
    /// Wraps a buffer as a layer.
    #[must_use]
    pub fn new(buffer: RingBuffer) -> Self {
        Self { buffer }
    }
}

impl<S: Subscriber> Layer<S> for RingLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let meta = event.metadata();
        let mut visitor = LineVisitor::default();
        event.record(&mut visitor);
        let line = format!(
            "{:>5} {}: {}",
            meta.level(),
            meta.target(),
            visitor.text.trim()
        );
        self.buffer.push(line);
    }
}

#[derive(Default)]
struct LineVisitor {
    text: String,
}

impl Visit for LineVisitor {
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            let _ = write!(self.text, "{value} ");
        } else {
            let _ = write!(self.text, "{}={value} ", field.name());
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        // Debug is the catch-all; secret types render redacted here (their custom Debug).
        if field.name() == "message" {
            let _ = write!(self.text, "{value:?} ");
        } else {
            let _ = write!(self.text, "{}={value:?} ", field.name());
        }
    }
}

/// Configuration for the logging pipeline.
#[derive(Debug, Clone)]
pub struct LogConfig {
    /// Initial `EnvFilter` directives (e.g. `"info"`, `"spidola::db=debug,info"`).
    pub default_directives: String,
    /// Ring-buffer capacity.
    pub ring_capacity: usize,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            default_directives: "info".to_owned(),
            ring_capacity: DEFAULT_RING_CAPACITY,
        }
    }
}

/// A reloadable `EnvFilter` directive setter, hiding the reload handle's concrete type.
type DirectiveSetter = Box<dyn Fn(&str) -> Result<(), String> + Send + Sync>;

/// A handle to the initialized pipeline: export the ring buffer and reload the level.
pub struct LogHandle {
    ring: RingBuffer,
    set_directives: DirectiveSetter,
}

impl LogHandle {
    /// Snapshots the recent log lines for export.
    #[must_use]
    pub fn export_logs(&self) -> Vec<String> {
        self.ring.export()
    }

    /// The ring buffer handle (for wiring additional consumers).
    #[must_use]
    pub fn ring(&self) -> &RingBuffer {
        &self.ring
    }

    /// Reloads the runtime log level from `EnvFilter` directives.
    ///
    /// # Errors
    /// Returns the parse/reload error as a string if `directives` are invalid.
    pub fn set_directives(&self, directives: &str) -> Result<(), String> {
        (self.set_directives)(directives)
    }
}

/// Initializes the global tracing pipeline (best-effort: a no-op if one is already set).
///
/// Returns a [`LogHandle`] for export and runtime level control. Intended to be called once
/// at library initialization in `core-api`.
#[must_use]
pub fn init(config: &LogConfig) -> LogHandle {
    let ring = RingBuffer::new(config.ring_capacity);
    let env = EnvFilter::new(&config.default_directives);
    let (filter_layer, reload_handle) = reload::Layer::new(env);
    let subscriber = Registry::default()
        .with(filter_layer)
        .with(RingLayer::new(ring.clone()));
    // Best-effort: another subscriber (or a prior init) may already own the global slot.
    let _ = subscriber.try_init();
    let set_directives = Box::new(move |directives: &str| {
        let filter = EnvFilter::try_new(directives).map_err(|e| e.to_string())?;
        reload_handle.reload(filter).map_err(|e| e.to_string())
    });
    LogHandle {
        ring,
        set_directives,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use core_model::Secret;
    use tracing::subscriber::with_default;

    fn capture(emit: impl FnOnce()) -> Vec<String> {
        let ring = RingBuffer::new(64);
        let subscriber = Registry::default().with(RingLayer::new(ring.clone()));
        with_default(subscriber, emit);
        ring.export()
    }

    #[test]
    fn events_land_in_the_ring_with_target_and_level() {
        let lines = capture(|| {
            tracing::info!(target: targets::IMPORT, channels = 42, "import complete");
        });
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("INFO"));
        assert!(lines[0].contains("spidola::import"));
        assert!(lines[0].contains("import complete"));
        assert!(lines[0].contains("channels=42"));
    }

    #[test]
    fn secrets_are_redacted_in_the_ring() {
        let lines = capture(|| {
            let secret = Secret::new("hunter2-top-secret");
            tracing::warn!(target: targets::FETCH, credential = ?secret, "auth attempt");
        });
        assert_eq!(lines.len(), 1);
        assert!(
            lines[0].contains("REDACTED"),
            "secret debug should be redacted"
        );
        assert!(
            !lines[0].contains("hunter2"),
            "raw secret leaked into the log ring: {}",
            lines[0]
        );
    }

    #[test]
    fn ring_buffer_evicts_oldest_past_capacity() {
        let ring = RingBuffer::new(3);
        for i in 0..5 {
            ring.push(format!("line {i}"));
        }
        let lines = ring.export();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines, vec!["line 2", "line 3", "line 4"]);
    }
}
