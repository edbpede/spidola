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
use std::sync::{Arc, Mutex, OnceLock, PoisonError, RwLock};

use thiserror::Error;
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
    /// Xtream account handshake and catalog mapping.
    pub const XTREAM: &str = "spidola::xtream";
    /// The LAN pairing micro-server.
    pub const PAIR: &str = "spidola::pair";
}

/// Severity of a forwarded log record, mapped one-to-one from `tracing::Level`.
///
/// `Info` is the [`Default`]: it is what the diagnostics screen's log level resolves to when the
/// user has never chosen one (PRD §6.9) — verbose enough to tell a support thread what happened,
/// quiet enough to cost nothing on the zap path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, uniffi::Enum)]
pub enum LogLevel {
    /// `tracing::Level::ERROR`.
    Error,
    /// `tracing::Level::WARN`.
    Warn,
    /// `tracing::Level::INFO`.
    #[default]
    Info,
    /// `tracing::Level::DEBUG`.
    Debug,
    /// `tracing::Level::TRACE`.
    Trace,
}

impl From<&tracing::Level> for LogLevel {
    fn from(level: &tracing::Level) -> Self {
        match *level {
            tracing::Level::ERROR => Self::Error,
            tracing::Level::WARN => Self::Warn,
            tracing::Level::INFO => Self::Info,
            tracing::Level::DEBUG => Self::Debug,
            tracing::Level::TRACE => Self::Trace,
        }
    }
}

impl LogLevel {
    /// The `EnvFilter` directive selecting this level globally, for
    /// [`LogHandle::set_directives`] (the diagnostics screen's log-level control, §4.8).
    ///
    /// Deliberately distinct from the level's *stored* spelling in
    /// [`crate::settings`]: one is this crate's contract with `tracing`, the other is the
    /// settings table's on-disk format, and they are free to diverge without breaking
    /// each other.
    #[must_use]
    pub(crate) fn directive(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warn => "warn",
            Self::Info => "info",
            Self::Debug => "debug",
            Self::Trace => "trace",
        }
    }
}

/// One log event forwarded to the host sink. Secret values are absent by construction (the
/// secret types redact their `Debug`), so a record never carries credential material (§12).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct LogRecord {
    /// Severity.
    pub level: LogLevel,
    /// The event's target (one of [`targets`]), which the shell maps to a native category.
    pub target: String,
    /// The rendered message plus structured fields.
    pub message: String,
}

/// The host log sink (`OSLog` on tvOS, logcat on Android, TECH_SPEC §4.8).
///
/// Foreign-implemented only, so this is a UniFFI callback interface. **Threading contract:**
/// [`Self::log`] is invoked synchronously from whichever core thread emitted the event and may
/// arrive on *any* thread; the shell forwards it to its platform logger without blocking.
#[uniffi::export(callback_interface)]
pub trait LogSink: Send + Sync {
    /// Forwards one log record to the platform logger.
    fn log(&self, record: LogRecord);
}

/// The installed host sink, owned by the live [`Core`] while it exists. Read on every (already
/// level-filtered) event, so an absent sink or a disabled level costs a single lock read.
///
/// The slot holds at most one owner: `install_sink` refuses a second install and [`clear_sink`]
/// releases it on `Core` drop. A single process-global [`SinkLayer`] routes *every* event to
/// whatever is here, so silently replacing a live owner would redirect the first `Core`'s
/// still-running events to the second host's callback — hence the guard.
static SINK: RwLock<Option<Arc<dyn LogSink>>> = RwLock::new(None);

/// The host-sink slot is already owned by a live [`Core`]. Surfaced instead of clobbering it,
/// so a second in-process `Core` cannot silently redirect the first's events to its own callback.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("a host log sink is already installed by a live Core")]
pub struct SinkInUse;

/// Installs the host log sink. Called once from the core constructor after the pipeline is
/// initialized; the [`SinkLayer`] added at init forwards subsequent events to it.
///
/// # Errors
/// Returns [`SinkInUse`] if a sink is already installed (a live `Core` owns the slot). The slot
/// is released by [`clear_sink`] when that `Core` drops, so a `Core` constructed afterward
/// installs cleanly.
pub fn install_sink(sink: Arc<dyn LogSink>) -> Result<(), SinkInUse> {
    let mut guard = SINK.write().unwrap_or_else(PoisonError::into_inner);
    if guard.is_some() {
        return Err(SinkInUse);
    }
    *guard = Some(sink);
    Ok(())
}

/// Releases the host-sink slot. Called from `Core`'s `Drop` so a torn-down host's callback is not
/// retained and a subsequently constructed `Core` can install its own sink.
pub fn clear_sink() {
    let mut guard = SINK.write().unwrap_or_else(PoisonError::into_inner);
    *guard = None;
}

/// A `tracing` layer that forwards each event to the installed host [`LogSink`].
struct SinkLayer;

impl<S: Subscriber> Layer<S> for SinkLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let guard = SINK.read().unwrap_or_else(PoisonError::into_inner);
        let Some(sink) = guard.as_ref() else {
            return; // no host sink yet (pre-init events); the ring buffer still captured it
        };
        let meta = event.metadata();
        let mut visitor = LineVisitor::default();
        event.record(&mut visitor);
        sink.log(LogRecord {
            level: LogLevel::from(meta.level()),
            target: meta.target().to_owned(),
            message: visitor.text.trim().to_owned(),
        });
    }
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
/// `Arc` (not `Box`) so [`LogHandle`] is `Clone`: every `init` caller shares the one setter
/// bound to the live subscriber.
type DirectiveSetter = Arc<dyn Fn(&str) -> Result<(), String> + Send + Sync>;

/// A handle to the initialized pipeline: export the ring buffer and reload the level.
#[derive(Clone)]
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

/// The reason [`init`] could not install the global pipeline.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LogInitError {
    /// A global `tracing` subscriber was already installed (by this process or the host) before
    /// the first `init`, so this pipeline never became the active dispatcher: its ring buffer and
    /// level control are detached from live events. The failure is surfaced rather than returning
    /// a live-looking handle.
    #[error("a global tracing subscriber is already installed: {0}")]
    AlreadyInstalled(String),
}

/// The one pipeline outcome: computed on the first [`init`] call, shared by every later one.
static LOG_HANDLE: OnceLock<Result<LogHandle, LogInitError>> = OnceLock::new();

/// Initializes the global tracing pipeline and returns a [`LogHandle`] for export and runtime
/// level control. Intended to be called once at library initialization in `core-api`.
///
/// Idempotent: the first call builds and installs the pipeline and caches its outcome; every
/// later call returns a clone of that same cached outcome (its `config` is ignored), so a second
/// caller never builds its own competing, detached pipeline.
///
/// # Errors
/// Returns [`LogInitError::AlreadyInstalled`] if a global `tracing` subscriber was already
/// installed when the first `init` ran. In that case the returned handle would be detached (empty
/// exports, no-op level control), so the failure is surfaced instead of returning it.
pub fn init(config: &LogConfig) -> Result<LogHandle, LogInitError> {
    LOG_HANDLE
        .get_or_init(|| {
            let ring = RingBuffer::new(config.ring_capacity);
            let env = EnvFilter::new(&config.default_directives);
            let (filter_layer, reload_handle) = reload::Layer::new(env);
            let subscriber = Registry::default()
                .with(filter_layer)
                .with(RingLayer::new(ring.clone()))
                // The host sink is forwarded to here; it reads the global slot set later by
                // `install_sink`, so init order (pipeline first, sink after) is not a race.
                .with(SinkLayer);
            // Surface a lost global-slot race rather than caching a detached, live-looking handle;
            // `OnceLock` makes this first outcome authoritative for every later caller.
            subscriber
                .try_init()
                .map_err(|e| LogInitError::AlreadyInstalled(e.to_string()))?;
            let set_directives: DirectiveSetter = Arc::new(move |directives: &str| {
                let filter = EnvFilter::try_new(directives).map_err(|e| e.to_string())?;
                reload_handle.reload(filter).map_err(|e| e.to_string())
            });
            Ok(LogHandle {
                ring,
                set_directives,
            })
        })
        .clone()
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
    fn init_returns_the_same_live_handle_across_calls() {
        // Nothing else in this test binary installs a *global* subscriber (the other tests use the
        // scoped `with_default`), so the first `init` wins the global slot and returns a live
        // handle. A second `init` must return a clone of that same handle — sharing the one live
        // ring, so a line pushed via the first is visible through the second's `export_logs`
        // (proving it is not a fresh, detached buffer).
        let cfg = LogConfig::default();
        let first = init(&cfg).expect("first init installs the global subscriber");
        let second = init(&cfg).expect("second init returns the cached live handle");
        first.ring().push("probe line".to_owned());
        assert!(second.export_logs().iter().any(|l| l == "probe line"));
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
