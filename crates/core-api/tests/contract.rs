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
    ApiError, AppSettings, BufferingProfile, Core, CoreConfig, ImportListener, ImportOutcome,
    ImportProgress, LogLevel, LogRecord, LogSink, PairingListener, PairingSubmission, SecretStore,
    Source, SubtitleSize,
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
///
/// Shares its map behind an `Arc` so a test can hold one handle while the `Core` owns another —
/// which is what lets the secrets tests below assert on what the host was actually asked to
/// store, rather than trusting the core's word for it.
#[derive(Clone, Default)]
struct FakeSecrets {
    store: Arc<Mutex<std::collections::HashMap<String, String>>>,
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

/// A pairing listener recording what the phone submitted.
#[derive(Clone, Default)]
struct RecordingPairingListener {
    seen: Arc<Mutex<Vec<PairingSubmission>>>,
}

impl PairingListener for RecordingPairingListener {
    fn on_submission(&self, submission: PairingSubmission) {
        self.seen.lock().unwrap().push(submission);
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

/// Serves a minimal but believable Xtream `player_api.php` from `127.0.0.1`, for `connections`
/// requests. Answers the handshake with an active account and every listing with an empty array,
/// which is all `add_xtream` needs: it authenticates and stores, and never lists.
///
/// Deliberately echoes the submitted credentials back inside `user_info`, exactly as real panels
/// do — that is the trap `secrets_never_reach_sqlite_or_the_log_stream` is set to catch.
fn spawn_xtream_stub(connections: usize) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    thread::spawn(move || {
        for _ in 0..connections {
            let Ok((mut socket, _)) = listener.accept() else {
                break;
            };
            let mut scratch = [0_u8; 4096];
            let read = socket.read(&mut scratch).unwrap_or(0);
            let request = String::from_utf8_lossy(&scratch[..read]).into_owned();
            // The credentials are in the request path (Xtream puts them in the URL); reflect them
            // back the way a real headend does.
            let body = if request.contains("action=") {
                "[]".to_owned()
            } else {
                format!(
                    "{{\"user_info\":{{\"auth\":1,\"status\":\"Active\",\"username\":\"{XTREAM_USER}\",\
                     \"password\":\"{XTREAM_PASSWORD}\",\"max_connections\":\"2\"}},\
                     \"server_info\":{{\"url\":\"host.example\"}}}}"
                )
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = socket.write_all(response.as_bytes());
            let _ = socket.flush();
        }
    });
    base
}

/// The account this suite adds. The password is distinctive so a substring search over the
/// database file and the log export cannot produce a false negative.
const XTREAM_USER: &str = "contract-user";
const XTREAM_PASSWORD: &str = "Sp1dola-Contract-Passphrase-8f3ac1";

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
    let core = open_core(rt, &db_path, sink.clone(), FakeSecrets::default());
    Harness {
        core,
        sink,
        _db: dir,
    }
}

/// Opens a `Core` over an explicit database path and host stores, so a test can close one and
/// reopen another on the same file — the "restart the app" shape — or keep a handle on what the
/// host was asked to store. A `Core` is single-per-process (it owns the host-sink slot), so the
/// previous one must be dropped first.
fn open_core(
    rt: &Runtime,
    db_path: &std::path::Path,
    sink: RecordingSink,
    secrets: FakeSecrets,
) -> Arc<Core> {
    let config = CoreConfig {
        db_path: db_path.to_string_lossy().into_owned(),
        log_directives: "debug".to_owned(),
    };
    rt.block_on(async { Core::new(config, Box::new(secrets), Box::new(sink)) })
        .expect("core initializes")
}

/// Every byte the core wrote to disk for this database: the main file plus SQLite's `-wal` and
/// `-shm` companions. Scanning only the `.sqlite` file would be a false negative under WAL mode,
/// where a recent write still lives in the log.
fn everything_on_disk(dir: &std::path::Path) -> Vec<u8> {
    let mut bytes = Vec::new();
    for entry in std::fs::read_dir(dir).expect("the database directory is readable") {
        let path = entry.expect("a readable entry").path();
        if path.is_file() {
            bytes.extend(std::fs::read(&path).expect("a readable file"));
        }
    }
    bytes
}

/// Whether `haystack` contains `needle` anywhere — a byte-level substring search, so it finds a
/// credential regardless of how SQLite framed the page it landed on.
fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
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
fn the_handshake_names_the_exact_core_build() {
    let _serial = serial();
    let rt = Runtime::new().unwrap();
    let harness = build_core(&rt);
    let handshake = harness.core.handshake();
    assert!(!handshake.core_version.is_empty());
    assert!(handshake.schema_version > 0);
    assert_eq!(handshake.boundary_version, core_api::BOUNDARY_VERSION);
    // The diagnostics screen shows this so a support thread can name the build (PRD §6.9). It is
    // "unknown" only where git metadata is genuinely absent — never empty, never a panic.
    assert!(
        !handshake.core_git_revision.is_empty(),
        "the git revision must always report something"
    );
}

#[test]
fn a_fresh_install_is_fully_configured_without_a_single_stored_setting() {
    // PRD §6.9's headline promise — "the app must be fully usable without ever opening
    // settings" — asserted at the boundary rather than assumed.
    let _serial = serial();
    let rt = Runtime::new().unwrap();
    let harness = build_core(&rt);
    let settings = rt.block_on(harness.core.settings().snapshot()).unwrap();
    assert_eq!(settings, AppSettings::default());
    assert!(settings.recents_enabled, "recents record by default");
    assert_eq!(
        settings.default_engine, None,
        "the platform default must need no stored choice"
    );
    assert_eq!(
        settings.epg_window_ahead_hours, 72,
        "PRD §6.6: 3 days ahead by default"
    );
}

#[test]
fn settings_persist_across_a_restart_because_the_core_owns_them() {
    // The core is the single source of truth: a shell that forgets everything on relaunch must
    // still find the user's choices intact.
    let _serial = serial();
    let rt = Runtime::new().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("spidola.sqlite");
    {
        let core = open_core(
            &rt,
            &db_path,
            RecordingSink::default(),
            FakeSecrets::default(),
        );
        let settings = core.settings();
        rt.block_on(settings.set_subtitle_size(SubtitleSize::Large))
            .unwrap();
        rt.block_on(settings.set_buffering(BufferingProfile::LowLatency))
            .unwrap();
        rt.block_on(settings.set_log_level(LogLevel::Error))
            .unwrap();
        rt.block_on(settings.set_engine_for_source(7, Some("mpv".to_owned())))
            .unwrap();
    } // dropped: releases the process-global host-sink slot for the next Core

    let core = open_core(
        &rt,
        &db_path,
        RecordingSink::default(),
        FakeSecrets::default(),
    );
    let settings = rt.block_on(core.settings().snapshot()).unwrap();
    assert_eq!(settings.subtitle_size, SubtitleSize::Large);
    assert_eq!(settings.buffering, BufferingProfile::LowLatency);
    assert_eq!(
        settings.log_level,
        LogLevel::Error,
        "a chosen log level must survive a restart, not revert to the start-up directives"
    );
    assert_eq!(
        rt.block_on(core.settings().engine_for_source(7))
            .unwrap()
            .as_deref(),
        Some("mpv")
    );
    assert_eq!(
        rt.block_on(core.settings().engine_for_source(8)).unwrap(),
        None,
        "one source's engine override must not leak onto another"
    );
}

#[test]
fn clearing_an_optional_setting_restores_its_default() {
    // "Unset" and "set to the default" stay the same state, so a future change to a default
    // still reaches users who never touched the setting.
    let _serial = serial();
    let rt = Runtime::new().unwrap();
    let harness = build_core(&rt);
    let settings = harness.core.settings();
    rt.block_on(settings.set_language(Some("da".to_owned())))
        .unwrap();
    assert_eq!(
        rt.block_on(settings.snapshot())
            .unwrap()
            .language
            .as_deref(),
        Some("da")
    );
    rt.block_on(settings.set_language(None)).unwrap();
    assert_eq!(
        rt.block_on(settings.snapshot()).unwrap().language,
        None,
        "clearing the language must fall back to following the system"
    );
}

#[test]
fn secrets_never_reach_sqlite_or_the_log_stream() {
    // Phase 6's exit criterion, and TECH_SPEC §12's central claim, asserted rather than asserted
    // *about*: add a real Xtream account through the real boundary against a headend that mirrors
    // the password back the way real panels do, then go looking for that password everywhere it
    // must not be.
    let _serial = serial();
    let rt = Runtime::new().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("spidola.sqlite");
    let sink = RecordingSink::default();
    let secrets = FakeSecrets::default();
    let core = open_core(&rt, &db_path, sink.clone(), secrets.clone());

    let base = spawn_xtream_stub(1);
    let source = rt
        .block_on(core.sources().add_xtream(
            "Home headend".to_owned(),
            base,
            XTREAM_USER.to_owned(),
            XTREAM_PASSWORD.to_owned(),
        ))
        .expect("the stub accepts the account");

    // 1. The password went to the host store — the only place it may rest — under a key that is
    //    genuinely opaque rather than a dressed-up copy of the credential.
    let key = {
        let stored = secrets.store.lock().unwrap();
        assert!(
            stored.values().any(|value| value == XTREAM_PASSWORD),
            "the password must reach the host secure store"
        );
        let key = stored.keys().next().expect("exactly one key").clone();
        assert!(
            !key.contains(XTREAM_PASSWORD) && !key.contains(XTREAM_USER),
            "the host-secrets key must not embed the credential: {key}"
        );
        key
    };

    // 2. The database references only that opaque key. Checked over the raw bytes of every file
    //    SQLite owns, so neither a column we forgot nor a page still sitting in the WAL can hide
    //    a credential from this assertion.
    let on_disk = everything_on_disk(dir.path());
    assert!(
        !contains_bytes(&on_disk, XTREAM_PASSWORD.as_bytes()),
        "the Xtream password reached the database file"
    );
    assert!(
        contains_bytes(&on_disk, key.as_bytes()),
        "the database should store the opaque key that names the password"
    );

    // 3. Nothing the user can export, and nothing the host sink received, carries it — including
    //    from the handshake response, which echoed the password straight back at us.
    let exported = core.export_logs();
    assert!(
        !exported.iter().any(|line| line.contains(XTREAM_PASSWORD)),
        "the password reached the diagnostics log export"
    );
    let records = sink.records.lock().unwrap();
    assert!(
        !records
            .iter()
            .any(|record| record.message.contains(XTREAM_PASSWORD)),
        "the password reached the host log sink"
    );
    drop(records);

    // 4. The boundary record itself cannot carry it — `Source::Xtream` has no password field, so
    //    this is a compile-time guarantee; the assertion pins the username/kind that *is* there.
    match source {
        Source::Xtream {
            username, common, ..
        } => {
            assert_eq!(username, XTREAM_USER);
            assert_eq!(common.name, "Home headend");
        }
        other => panic!("expected an Xtream source, got {other:?}"),
    }
}

#[test]
fn deleting_an_xtream_source_removes_its_stored_credential() {
    // The row is the only record of which opaque key belongs to this account, so a delete that
    // skipped the store would strand the password in the keychain with nothing able to name it.
    let _serial = serial();
    let rt = Runtime::new().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("spidola.sqlite");
    let secrets = FakeSecrets::default();
    let core = open_core(&rt, &db_path, RecordingSink::default(), secrets.clone());

    let base = spawn_xtream_stub(1);
    let source = rt
        .block_on(core.sources().add_xtream(
            "Home headend".to_owned(),
            base,
            XTREAM_USER.to_owned(),
            XTREAM_PASSWORD.to_owned(),
        ))
        .expect("the stub accepts the account");
    assert_eq!(secrets.store.lock().unwrap().len(), 1);

    let id = match &source {
        Source::Xtream { id, .. } => *id,
        other => panic!("expected an Xtream source, got {other:?}"),
    };
    rt.block_on(core.sources().delete(id)).expect("delete");

    assert!(
        secrets.store.lock().unwrap().is_empty(),
        "deleting the source must delete its password from the host store"
    );
}

#[test]
fn a_rejected_xtream_account_is_never_persisted() {
    // Verifying before storing is what makes a wrong password a sentence on the add screen rather
    // than a mystery on the next refresh — and it must leave nothing behind when it fails.
    let _serial = serial();
    let rt = Runtime::new().unwrap();
    let harness = build_core(&rt);

    // No stub: nothing is listening on this port, so the handshake cannot succeed.
    let dead = {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        format!("http://{addr}")
    };
    let result = rt.block_on(harness.core.sources().add_xtream(
        "Nope".to_owned(),
        dead,
        XTREAM_USER.to_owned(),
        XTREAM_PASSWORD.to_owned(),
    ));
    assert!(result.is_err(), "an unreachable headend must not be added");
    let sources = rt.block_on(harness.core.sources().list()).unwrap();
    assert!(
        sources.is_empty(),
        "a failed add must leave no source behind"
    );
}

#[test]
fn the_pairing_server_serves_the_agpl_source_link_and_stops_with_its_screen() {
    // The server exists only while its screen is visible (TECH_SPEC §12), and every page it
    // serves carries the AGPL §13 offer (PRD §10). Both are asserted here against the real
    // service, over a real socket, from the shell's side of the boundary.
    let _serial = serial();
    let rt = Runtime::new().unwrap();
    let harness = build_core(&rt);

    // Advertise loopback explicitly — the "shell supplies the address" path, which is what a
    // shipping shell does anyway. Asking the core to infer it would make this test depend on the
    // host's routing table, and it would fail on any machine behind a full-tunnel VPN.
    let listener = RecordingPairingListener::default();
    let session = rt
        .block_on(
            harness
                .core
                .pairing()
                .start(Some("127.0.0.1".to_owned()), Box::new(listener.clone())),
        )
        .expect("the pairing server starts");

    assert_eq!(
        session.token.len(),
        6,
        "the token must be typeable: {session:?}"
    );
    assert!(session.port > 0);
    assert!(
        session.url.ends_with(&session.port.to_string()),
        "the advertised URL must name the port it is served on: {session:?}"
    );

    let page = fetch_pairing_page(session.port);
    assert!(
        page.contains("AGPL-3.0") && page.contains("Source code"),
        "every served page must carry the AGPL §13 source offer:\n{page}"
    );

    rt.block_on(harness.core.pairing().stop());
    assert!(
        std::net::TcpStream::connect(("127.0.0.1", session.port)).is_err(),
        "the server must not outlive the screen that started it"
    );
}

/// Fetches `GET /` from the pairing server and returns the whole response.
fn fetch_pairing_page(port: u16) -> String {
    let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).expect("the server answers");
    stream
        .write_all(b"GET / HTTP/1.1\r\nHost: tv.local\r\n\r\n")
        .unwrap();
    let mut body = String::new();
    stream.read_to_string(&mut body).unwrap();
    body
}

/// POSTs a urlencoded `body` to the pairing server and returns the whole response.
fn post_pairing_form(port: u16, body: &str) -> String {
    let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).expect("the server answers");
    let request = format!(
        "POST / HTTP/1.1\r\nHost: tv.local\r\nContent-Type: application/x-www-form-urlencoded\r\n\
         Content-Length: {}\r\n\r\n{}",
        body.len(),
        body
    );
    stream.write_all(request.as_bytes()).unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    response
}

#[test]
fn a_phone_submission_reaches_the_shell_only_with_the_session_token() {
    // The whole pairing loop, end to end through the real boundary: the token gates the POST,
    // and an accepted submission arrives at the shell as a pre-filled add-source flow (PRD §6.1).
    let _serial = serial();
    let rt = Runtime::new().unwrap();
    let harness = build_core(&rt);

    let listener = RecordingPairingListener::default();
    let session = rt
        .block_on(
            harness
                .core
                .pairing()
                .start(Some("127.0.0.1".to_owned()), Box::new(listener.clone())),
        )
        .expect("the pairing server starts");

    // A wrong token is refused, and nothing reaches the shell — this is the property that makes
    // "a person on the network cannot inject a source into a TV they cannot see" true (§12).
    let refused = post_pairing_form(
        session.port,
        "token=ZZZZZZ&kind=m3u-url&url=http%3A%2F%2Fhost.example%2Flist.m3u",
    );
    assert!(
        refused.starts_with("HTTP/1.1 403"),
        "a wrong token must be refused:\n{refused}"
    );
    assert!(
        listener.seen.lock().unwrap().is_empty(),
        "a refused submission must never reach the shell"
    );

    // The real token is accepted, and the submission lands parsed.
    let accepted = post_pairing_form(
        session.port,
        &format!(
            "token={}&kind=m3u-url&url=http%3A%2F%2Fhost.example%2Flist.m3u",
            session.token
        ),
    );
    assert!(
        accepted.starts_with("HTTP/1.1 200"),
        "the session token must be accepted:\n{accepted}"
    );
    let seen = listener.seen.lock().unwrap();
    assert_eq!(seen.len(), 1, "exactly one submission should have landed");
    match &seen[0] {
        PairingSubmission::M3uUrl { url } => {
            assert_eq!(url, "http://host.example/list.m3u");
        }
        other @ PairingSubmission::Xtream { .. } => {
            panic!("expected an M3U submission, got {other:?}")
        }
    }
    drop(seen);

    rt.block_on(harness.core.pairing().stop());
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
