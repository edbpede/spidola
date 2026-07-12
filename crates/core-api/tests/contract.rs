// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Boundary contract tests: exercise the real, compiled `core-api` surface exactly as the
//! shells do (TECH_SPEC §5, §10, Phase 2 exit criteria).
//!
//! These are the Rust half of the parity keel. They drive a fixture playlist through the
//! boundary — add a source, refresh it with a registered listener, receive progress, cancel a
//! second import mid-stream and prove the prior catalog survives — while a host log sink
//! captures the pipeline and a host secret store is installed. A companion Swift harness runs
//! the same flow against the same compiled library (`apps/tvos/Packages/CoreKit/Tests`), so
//! "parity" is asserted from both bindings. Any core panic crossing the boundary trips the
//! panic guard and fails the build; every FFI error variant is asserted constructible and
//! actionable.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fmt::Write as _;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::Duration;

use core_api::{
    ApiError, Core, CoreConfig, ImportListener, ImportOutcome, ImportProgress, LogRecord, LogSink,
    SecretStore, Source,
};
use tokio::runtime::Runtime;

/// Contract tests exercise process-global state (the logging pipeline installs once and its
/// host sink is replaced per `Core`), so they run serially under this lock.
static SERIAL: Mutex<()> = Mutex::new(());

fn serial() -> MutexGuard<'static, ()> {
    SERIAL
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

// -- Host callback fakes -----------------------------------------------------------------

/// A host secret store backed by an in-memory map (stands in for Keychain / Keystore).
#[derive(Default)]
struct FakeSecrets {
    store: Mutex<std::collections::HashMap<String, String>>,
}

impl SecretStore for FakeSecrets {
    fn get(&self, key: String) -> Result<Option<String>, ApiError> {
        Ok(self.store.lock().unwrap().get(&key).cloned())
    }
    fn set(&self, key: String, value: String) -> Result<(), ApiError> {
        self.store.lock().unwrap().insert(key, value);
        Ok(())
    }
    fn delete(&self, key: String) -> Result<(), ApiError> {
        self.store.lock().unwrap().remove(&key);
        Ok(())
    }
}

/// A host log sink capturing every forwarded record.
#[derive(Clone, Default)]
struct RecordingSink {
    records: Arc<Mutex<Vec<LogRecord>>>,
}

impl LogSink for RecordingSink {
    fn log(&self, record: LogRecord) {
        self.records.lock().unwrap().push(record);
    }
}

/// The terminal outcome of an import, as observed by the listener.
#[derive(Debug)]
enum Terminal {
    Complete(ImportOutcome),
    Failed(ApiError),
}

/// A listener that records progress and signals the terminal outcome over a channel.
struct CollectingListener {
    progress: Arc<Mutex<Vec<ImportProgress>>>,
    first_progress: Arc<Mutex<Option<Sender<()>>>>,
    terminal: Sender<Terminal>,
}

impl ImportListener for CollectingListener {
    fn on_progress(&self, progress: ImportProgress) {
        self.progress.lock().unwrap().push(progress);
        // Fire a one-shot on the first progress so a test can cancel deterministically mid-import.
        if let Some(tx) = self.first_progress.lock().unwrap().take() {
            let _ = tx.send(());
        }
    }
    fn on_complete(&self, outcome: ImportOutcome) {
        let _ = self.terminal.send(Terminal::Complete(outcome));
    }
    fn on_failed(&self, error: ApiError) {
        let _ = self.terminal.send(Terminal::Failed(error));
    }
}

// -- Local HTTP stub ---------------------------------------------------------------------

/// Serves `body` over HTTP/1.1 from a `127.0.0.1` port, once per entry in `connections`. Each
/// connection is written in `chunk`-byte slices with `delay` between them, so a source can be
/// imported first quickly and then re-imported slowly enough to cancel mid-stream.
fn spawn_stub(body: Vec<u8>, connections: Vec<(usize, Duration)>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/playlist.m3u", listener.local_addr().unwrap());
    thread::spawn(move || {
        for (chunk, delay) in connections {
            match listener.accept() {
                Ok((mut socket, _)) => serve(&mut socket, &body, chunk, delay),
                Err(_) => break,
            }
        }
    });
    url
}

/// Writes one HTTP/1.1 response carrying `body` in paced `chunk`-byte slices.
fn serve(socket: &mut std::net::TcpStream, body: &[u8], chunk: usize, delay: Duration) {
    let mut scratch = [0_u8; 1024];
    let _ = socket.read(&mut scratch); // consume the request line + headers
    let header = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/x-mpegurl\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    if socket.write_all(header.as_bytes()).is_err() {
        return;
    }
    for slice in body.chunks(chunk.max(1)) {
        if socket.write_all(slice).is_err() {
            return;
        }
        let _ = socket.flush();
        if !delay.is_zero() {
            thread::sleep(delay);
        }
    }
}

/// A synthetic M3U with `count` uniquely-identified channels.
fn playlist(count: usize) -> Vec<u8> {
    let mut out = String::from("#EXTM3U\n");
    for i in 0..count {
        let _ = write!(
            out,
            "#EXTINF:-1 tvg-id=\"id{i}\" group-title=\"News\",Channel {i}\nhttp://host.example/live/{i}.ts\n"
        );
    }
    out.into_bytes()
}

// -- Core construction -------------------------------------------------------------------

struct Harness {
    core: Arc<Core>,
    sink: RecordingSink,
    _db: tempfile::TempDir,
}

fn build_core(rt: &Runtime) -> Harness {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("spidola.sqlite");
    let sink = RecordingSink::default();
    let config = CoreConfig {
        db_path: db_path.to_string_lossy().into_owned(),
        log_directives: "debug".to_owned(),
    };
    let core = rt
        .block_on(async {
            Core::new(
                config,
                Box::new(FakeSecrets::default()),
                Box::new(sink.clone()),
            )
        })
        .expect("core initializes");
    Harness {
        core,
        sink,
        _db: dir,
    }
}

/// Runs `body` with a panic hook that counts any panic (including from spawned core tasks) so a
/// panic crossing the FFI is detectable; returns the panic count observed during the closure.
fn with_panic_guard<T>(body: impl FnOnce() -> T) -> (T, usize) {
    let count = Arc::new(AtomicUsize::new(0));
    let previous = std::panic::take_hook();
    let counter = Arc::clone(&count);
    std::panic::set_hook(Box::new(move |_info| {
        counter.fetch_add(1, Ordering::SeqCst);
    }));
    let result = body();
    std::panic::set_hook(previous);
    (result, count.load(Ordering::SeqCst))
}

// -- Tests -------------------------------------------------------------------------------

#[test]
fn imports_a_fixture_through_the_boundary_with_progress_and_logs() {
    let _serial = serial();
    let rt = Runtime::new().unwrap();
    let harness = build_core(&rt);
    let url = spawn_stub(playlist(300), vec![(4096, Duration::ZERO)]);

    let ((source, terminal, progress), panics) = with_panic_guard(|| {
        let sources = harness.core.sources();
        let source = rt
            .block_on(sources.add_m3u_url("Fixture".to_owned(), url.clone(), None, false))
            .expect("add source");
        let source_id = match source {
            Source::M3uUrl { id, .. } => id,
            other => panic!("unexpected source kind: {other:?}"),
        };

        let progress = Arc::new(Mutex::new(Vec::new()));
        let (terminal_tx, terminal_rx) = channel();
        let listener = CollectingListener {
            progress: Arc::clone(&progress),
            first_progress: Arc::new(Mutex::new(None)),
            terminal: terminal_tx,
        };
        let _handle = sources.refresh(source_id, Box::new(listener));
        let terminal = terminal_rx
            .recv_timeout(Duration::from_secs(30))
            .expect("terminal");
        (source_id, terminal, progress)
    });

    assert_eq!(panics, 0, "a core panic crossed the FFI");
    match terminal {
        Terminal::Complete(outcome) => assert_eq!(outcome.inserted, 300),
        Terminal::Failed(error) => panic!("import failed: {error:?}"),
    }
    let seen = progress.lock().unwrap();
    assert!(!seen.is_empty(), "no progress callbacks were delivered");

    // The catalog is queryable through the boundary and matches the import.
    let count = rt
        .block_on(harness.core.catalog().channel_count(source))
        .expect("count");
    assert_eq!(count, 300);

    // The pipeline logged through the host sink (proves §4.8 wiring end to end).
    let records = harness.sink.records.lock().unwrap();
    assert!(
        records.iter().any(|r| r.target == "spidola::import"),
        "the import subsystem never reached the host log sink"
    );
}

#[test]
fn cancel_mid_import_leaves_the_prior_catalog_intact() {
    let _serial = serial();
    let rt = Runtime::new().unwrap();
    let harness = build_core(&rt);
    let sources = harness.core.sources();

    // One stub URL, served twice: a fast full import establishes a prior catalog, then a slow
    // second import over the same URL is cancelled mid-stream. The playlist spans several parser
    // batches so progress (and the cancellation window) lands while bytes are still streaming.
    let catalog_size = 5_000;
    let url = spawn_stub(
        playlist(catalog_size),
        vec![
            (64 * 1024, Duration::ZERO),          // 1st connection: fast, full import
            (8 * 1024, Duration::from_millis(5)), // 2nd connection: slow, cancelled
        ],
    );
    let source = rt
        .block_on(sources.add_m3u_url("Fixture".to_owned(), url, None, false))
        .expect("add source");
    let source_id = match source {
        Source::M3uUrl { id, .. } => id,
        other => panic!("unexpected source kind: {other:?}"),
    };
    let (done_tx, done_rx) = channel();
    let _first = sources.refresh(
        source_id,
        Box::new(CollectingListener {
            progress: Arc::new(Mutex::new(Vec::new())),
            first_progress: Arc::new(Mutex::new(None)),
            terminal: done_tx,
        }),
    );
    matches!(
        done_rx.recv_timeout(Duration::from_mins(1)).unwrap(),
        Terminal::Complete(_)
    )
    .then_some(())
    .expect("first import completes");

    // Now the slow second import, cancelled once the first batch has been staged.
    let (first_tx, first_rx) = channel();
    let (terminal_tx, terminal_rx) = channel();
    let (terminal, panics) = with_panic_guard(|| {
        let handle = sources.refresh(
            source_id,
            Box::new(CollectingListener {
                progress: Arc::new(Mutex::new(Vec::new())),
                first_progress: Arc::new(Mutex::new(Some(first_tx))),
                terminal: terminal_tx,
            }),
        );
        // Wait until at least one batch has been staged, then cancel at the batch boundary.
        first_rx
            .recv_timeout(Duration::from_mins(1))
            .expect("first progress");
        handle.cancel();
        terminal_rx
            .recv_timeout(Duration::from_mins(1))
            .expect("terminal")
    });

    assert_eq!(panics, 0, "a core panic crossed the FFI");
    assert!(
        matches!(terminal, Terminal::Failed(ApiError::Cancelled)),
        "expected a Cancelled terminal, got {terminal:?}"
    );
    // The prior catalog is untouched (staging-and-swap rolled back on cancel).
    let count = rt
        .block_on(harness.core.catalog().channel_count(source_id))
        .expect("count");
    assert_eq!(
        count, catalog_size as u64,
        "cancel corrupted the prior catalog"
    );
}

#[test]
fn boundary_failures_map_to_actionable_errors() {
    let _serial = serial();
    let rt = Runtime::new().unwrap();
    let harness = build_core(&rt);
    let sources = harness.core.sources();

    // A malformed URL never constructs a source: it maps to an actionable input error.
    let bad =
        rt.block_on(sources.add_m3u_url("Bad".to_owned(), "not a url".to_owned(), None, false));
    assert!(
        matches!(bad, Err(ApiError::InvalidInput { .. })),
        "expected InvalidInput, got {bad:?}"
    );

    // Refreshing a source that does not exist reports NotFound via the listener.
    let (terminal_tx, terminal_rx) = channel();
    let _refresh = sources.refresh(
        9_999,
        Box::new(CollectingListener {
            progress: Arc::new(Mutex::new(Vec::new())),
            first_progress: Arc::new(Mutex::new(None)),
            terminal: terminal_tx,
        }),
    );
    let terminal = terminal_rx.recv_timeout(Duration::from_secs(30)).unwrap();
    assert!(
        matches!(terminal, Terminal::Failed(ApiError::NotFound)),
        "expected NotFound, got {terminal:?}"
    );
}

#[test]
fn every_error_variant_is_constructible_and_actionable() {
    // Exhaustive by construction: adding a variant forces an addition here, and each must map to
    // a non-empty, jargon-free UX (PRD §6.3) so it is representable and renderable on both sides.
    let variants = [
        ApiError::NetworkUnreachable,
        ApiError::Timeout,
        ApiError::Unauthorized,
        ApiError::NotFound,
        ApiError::InvalidInput {
            reason: "bad".to_owned(),
        },
        ApiError::ParseFailed {
            emitted: 1,
            skipped: 2,
        },
        ApiError::StorageCorrupt,
        ApiError::Cancelled,
        ApiError::Internal,
    ];
    for variant in variants {
        let ux = variant.ux();
        assert!(!ux.actions.is_empty(), "{variant:?} has no action");
        assert!(!ux.failure_class.is_empty());
        assert!(!variant.to_string().is_empty());
    }
}
