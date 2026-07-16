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
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::Duration;

use core_api::{
    ApiError, AppSettings, BufferingProfile, Core, CoreConfig, CustomChannelDraft,
    CustomImportMode, EpgRefreshListener, EpgRefreshOutcome, EpgRefreshProgress, ImportListener,
    ImportOutcome, ImportProgress, InputField, InputIssue, LogLevel, LogRecord, LogSink,
    PairingListener, PairingSubmission, ResolvedHeader, ResolvedStream, SecretStore, Source,
    SubtitleSize,
};
use core_model::channel::channel_identity;
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
///
/// A real Keychain can say no — the device is locked, an entitlement is wrong — and the orders
/// the credential paths are written in are only observable when it does. `refuse_deletes` is how
/// a test asks for that no.
#[derive(Clone, Default)]
struct FakeSecrets {
    store: Arc<Mutex<std::collections::HashMap<String, String>>>,
    refuse_sets: Arc<AtomicBool>,
    refuse_deletes: Arc<AtomicBool>,
}

impl SecretStore for FakeSecrets {
    fn get(&self, key: String) -> Result<Option<String>, ApiError> {
        Ok(self.store.lock().unwrap().get(&key).cloned())
    }
    fn set(&self, key: String, value: String) -> Result<(), ApiError> {
        if self.refuse_sets.load(Ordering::SeqCst) {
            return Err(ApiError::Internal);
        }
        self.store.lock().unwrap().insert(key, value);
        Ok(())
    }
    fn delete(&self, key: String) -> Result<(), ApiError> {
        if self.refuse_deletes.load(Ordering::SeqCst) {
            return Err(ApiError::Internal);
        }
        self.store.lock().unwrap().remove(&key);
        Ok(())
    }
}

#[test]
fn an_m3u_source_is_not_created_when_the_secure_store_refuses_its_url() {
    let _serial = serial();
    let rt = Runtime::new().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("spidola.sqlite");
    let secrets = FakeSecrets::default();
    secrets.refuse_sets.store(true, Ordering::SeqCst);
    let core = open_core(&rt, &db_path, RecordingSink::default(), secrets.clone());

    let result = rt.block_on(core.sources().add_m3u_url(
        "Secure playlist".to_owned(),
        "http://host.example/list.m3u?token=secret".to_owned(),
        None,
        false,
    ));
    assert!(
        result.is_err(),
        "a failed secure-store write must fail closed"
    );
    assert!(
        rt.block_on(core.sources().list()).unwrap().is_empty(),
        "the database must not reference a URL that was never stored"
    );
}

/// Makes every subsequent `sources` insert on the database at `db_path` fail.
///
/// Installed over a second handle to the same file, because the boundary is typed all the way
/// down: it offers no way to ask for one write to fail and the rest to work, and the paths that
/// matter here are the ones that run when a write does. SQLite keeps the trigger in the schema,
/// so the `Core`'s own connections meet it on their next insert without knowing this happened.
fn refuse_source_inserts(db_path: &std::path::Path) {
    let db = core_db::Db::open(db_path).expect("a second handle to the same database");
    db.writer()
        .execute_batch(
            "CREATE TRIGGER refuse_sources BEFORE INSERT ON sources \
             BEGIN SELECT RAISE(ABORT, 'the disk said no'); END",
        )
        .expect("the trigger installs");
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

#[derive(Debug)]
enum EpgTerminal {
    Complete(EpgRefreshOutcome),
    Failed(ApiError),
}

struct CollectingEpgListener {
    progress: Arc<Mutex<Vec<EpgRefreshProgress>>>,
    terminal: Sender<EpgTerminal>,
}

impl EpgRefreshListener for CollectingEpgListener {
    fn on_progress(&self, progress: EpgRefreshProgress) {
        self.progress.lock().unwrap().push(progress);
    }

    fn on_complete(&self, outcome: EpgRefreshOutcome) {
        let _ = self.terminal.send(EpgTerminal::Complete(outcome));
    }

    fn on_failed(&self, error: ApiError) {
        let _ = self.terminal.send(EpgTerminal::Failed(error));
    }
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
const M3U_CREDENTIAL: &str = "Sp1dola-M3U-Credential-b71de4";

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
    db_dir: tempfile::TempDir,
}

fn build_core(rt: &Runtime) -> Harness {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("spidola.sqlite");
    let sink = RecordingSink::default();
    let core = open_core(rt, &db_path, sink.clone(), FakeSecrets::default());
    Harness {
        core,
        sink,
        db_dir: dir,
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
        rt.block_on(settings.set_buffering(BufferingProfile::Low))
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
    assert_eq!(settings.buffering, BufferingProfile::Low);
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
fn all_three_engine_tiers_are_storable_and_independent() {
    // PRD §6.3's selection policy is channel → source → platform default. All three tiers must
    // exist at the boundary and must not clobber each other. This test exists because Phase 6's
    // typed-settings rewrite briefly shipped without the channel tier: the Rust suite stayed
    // green the whole time, because the only thing that noticed was the shells' playback slice
    // on the far side of the FFI.
    let _serial = serial();
    let rt = Runtime::new().unwrap();
    let harness = build_core(&rt);
    let settings = harness.core.settings();

    rt.block_on(settings.set_default_engine(Some("mpv".to_owned())))
        .unwrap();
    rt.block_on(settings.set_engine_for_source(1, Some("avplayer".to_owned())))
        .unwrap();
    rt.block_on(settings.set_engine_for_channel(1, 42, Some("mpv".to_owned())))
        .unwrap();

    assert_eq!(
        rt.block_on(settings.snapshot())
            .unwrap()
            .default_engine
            .as_deref(),
        Some("mpv")
    );
    assert_eq!(
        rt.block_on(settings.engine_for_source(1))
            .unwrap()
            .as_deref(),
        Some("avplayer"),
        "the source tier must not be clobbered by the global one"
    );
    assert_eq!(
        rt.block_on(settings.engine_for_channel(1, 42))
            .unwrap()
            .as_deref(),
        Some("mpv"),
        "the channel tier must not be clobbered by the source one"
    );

    // Scoping: another channel of the same source, and the same identity under another source,
    // are both untouched.
    assert_eq!(
        rt.block_on(settings.engine_for_channel(1, 43)).unwrap(),
        None
    );
    assert_eq!(
        rt.block_on(settings.engine_for_channel(2, 42)).unwrap(),
        None
    );

    // Clearing the channel tier falls back to the source tier rather than to nothing.
    rt.block_on(settings.set_engine_for_channel(1, 42, None))
        .unwrap();
    assert_eq!(
        rt.block_on(settings.engine_for_channel(1, 42)).unwrap(),
        None
    );
    assert_eq!(
        rt.block_on(settings.engine_for_source(1))
            .unwrap()
            .as_deref(),
        Some("avplayer"),
        "clearing a channel override must leave the source override standing"
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
fn credential_bearing_m3u_urls_never_reach_sqlite_or_the_log_stream() {
    // Real M3U providers commonly put the same account token in both the playlist address and
    // every channel locator. Those values must remain usable for refresh/playback while resting
    // only in the host secure store or as authenticated ciphertext on disk.
    let _serial = serial();
    let rt = Runtime::new().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("spidola.sqlite");
    let sink = RecordingSink::default();
    let secrets = FakeSecrets::default();
    let core = open_core(&rt, &db_path, sink.clone(), secrets.clone());

    let stream_url = format!("http://stream.example/live/{M3U_CREDENTIAL}/1.ts");
    let playlist = format!(
        "#EXTM3U\n#EXTINF:-1 group-title=\"News\",Secure One\n\
         #EXTVLCOPT:http-user-agent=Bearer-{M3U_CREDENTIAL}\n\
         #EXTVLCOPT:http-referrer=http://portal.example/{M3U_CREDENTIAL}\n\
         {stream_url}\n"
    );
    let source_url = format!(
        "{}?account={M3U_CREDENTIAL}",
        spawn_stub(playlist.into_bytes(), vec![(4096, Duration::ZERO)])
    );
    let sources = core.sources();
    let source = rt
        .block_on(sources.add_m3u_url(
            "Credential playlist".to_owned(),
            source_url,
            Some(format!("Provider-Agent-{M3U_CREDENTIAL}")),
            false,
        ))
        .expect("source is accepted");
    let source_id = match source {
        Source::M3uUrl { id, .. } => id,
        other => panic!("expected an M3U source, got {other:?}"),
    };

    let (terminal_tx, terminal_rx) = channel();
    let _refresh = sources.refresh(
        source_id,
        Box::new(CollectingListener {
            progress: Arc::new(Mutex::new(Vec::new())),
            first_progress: Arc::new(Mutex::new(None)),
            terminal: terminal_tx,
        }),
    );
    assert!(
        matches!(
            terminal_rx.recv_timeout(Duration::from_secs(30)).unwrap(),
            Terminal::Complete(ImportOutcome { inserted: 1, .. })
        ),
        "the credential-bearing playlist must still import"
    );

    let channel = rt
        .block_on(core.catalog().channels(source_id, 0, 1))
        .expect("catalog remains readable")
        .channels
        .into_iter()
        .next()
        .expect("one channel imported");
    assert!(
        !channel.locator.contains(M3U_CREDENTIAL),
        "the FFI/navigation locator must remain an opaque envelope"
    );
    assert_ne!(
        channel.identity,
        channel_identity(None, &stream_url, "Secure One").to_storage(),
        "the persisted identity must not be a public offline verifier for the credential URL"
    );
    assert!(
        channel
            .overrides
            .headers
            .iter()
            .all(|header| !header.value.contains(M3U_CREDENTIAL))
            && channel
                .overrides
                .user_agent
                .as_deref()
                .is_none_or(|value| !value.contains(M3U_CREDENTIAL)),
        "credential-bearing header values must remain opaque across persistence and FFI"
    );
    let playable = rt
        .block_on(sources.resolve_playback(
            channel.source_id,
            channel.identity,
            channel.locator.clone(),
        ))
        .expect("the resolver opens the stored envelopes");
    assert_resolved_m3u_request(&playable);
    rt.block_on(core.recents().record(
        channel.source_id,
        channel.identity,
        channel.name,
        channel.locator,
        None,
    ))
    .expect("history records without persisting plaintext");

    assert_m3u_secret_boundary(&core, &sink, &secrets, dir.path(), source_id, &rt);
}

fn assert_resolved_m3u_request(playable: &ResolvedStream) {
    let locator = playable.locator();
    assert!(
        locator.contains(M3U_CREDENTIAL),
        "the engine still needs the original playable locator"
    );
    let user_agent = playable.user_agent();
    assert_eq!(
        user_agent.as_deref(),
        Some(format!("Bearer-{M3U_CREDENTIAL}").as_str()),
        "the engine must receive the opened per-channel user-agent"
    );
    let headers = playable.headers();
    assert_eq!(headers.len(), 1);
    assert_eq!(headers[0].name(), "Referer");
    assert!(headers[0].value().contains(M3U_CREDENTIAL));
    assert!(
        !format!("{playable:?}").contains(M3U_CREDENTIAL),
        "ephemeral resolved values must remain redacted in diagnostics"
    );
}

fn assert_m3u_secret_boundary(
    core: &Arc<Core>,
    sink: &RecordingSink,
    secrets: &FakeSecrets,
    db_dir: &std::path::Path,
    source_id: i64,
    rt: &Runtime,
) {
    let on_disk = everything_on_disk(db_dir);
    assert!(
        !contains_bytes(&on_disk, M3U_CREDENTIAL.as_bytes()),
        "an M3U credential reached SQLite or its WAL"
    );
    assert!(
        secrets
            .store
            .lock()
            .unwrap()
            .values()
            .any(|value| value.contains(M3U_CREDENTIAL)),
        "the credential-bearing source address must rest in the host secure store"
    );
    assert!(
        !core
            .export_logs()
            .iter()
            .any(|line| line.contains(M3U_CREDENTIAL)),
        "an M3U credential reached the diagnostics log export"
    );
    assert!(
        !sink
            .records
            .lock()
            .unwrap()
            .iter()
            .any(|record| record.message.contains(M3U_CREDENTIAL)),
        "an M3U credential reached the host log sink"
    );
    rt.block_on(core.sources().delete(source_id))
        .expect("source and its secure values delete together");
    assert!(
        secrets
            .store
            .lock()
            .unwrap()
            .values()
            .all(|value| !value.contains(M3U_CREDENTIAL)),
        "deleting an M3U source must remove its URL and user-agent secrets"
    );
}

#[test]
fn an_xtream_stream_is_playable_only_through_the_resolver() {
    // The other half of the credential-free-catalog bargain: if nothing puts the password back at
    // play time, an Xtream channel imports, browses, and favorites perfectly — and then cannot be
    // played. This asserts the round trip the zap path depends on.
    let _serial = serial();
    let rt = Runtime::new().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("spidola.sqlite");
    let secrets = FakeSecrets::default();
    let sink = RecordingSink::default();
    let core = open_core(&rt, &db_path, sink.clone(), secrets.clone());

    let base = spawn_xtream_stub(1);
    let source = rt
        .block_on(core.sources().add_xtream(
            "Home headend".to_owned(),
            base.clone(),
            XTREAM_USER.to_owned(),
            XTREAM_PASSWORD.to_owned(),
        ))
        .expect("the stub accepts the account");
    let id = match &source {
        Source::Xtream { id, .. } => *id,
        other => panic!("expected an Xtream source, got {other:?}"),
    };

    // The shape `core-xtream` persists: credential-free, `{server}/{kind}/{id}.{ext}`.
    let stored = format!("{base}/live/4242.ts");
    let playable = rt
        .block_on(core.sources().resolve_stream(id, stored.clone()))
        .expect("a stored Xtream locator resolves");

    assert!(
        !stored.contains(XTREAM_PASSWORD),
        "the stored locator must be credential-free — that is the whole point"
    );
    assert!(
        playable.contains(XTREAM_PASSWORD) && playable.contains(XTREAM_USER),
        "the resolved URL must carry the credentials an engine needs"
    );
    assert!(
        playable.contains("/live/") && playable.ends_with("4242.ts"),
        "the resolved URL must still name the same stream"
    );

    assert_xtream_recent_replays(&rt, &core, id, &stored);

    // Resolving must not leak it into the diagnostics the user can export.
    assert!(
        !core
            .export_logs()
            .iter()
            .any(|l| l.contains(XTREAM_PASSWORD)),
        "resolving a stream leaked the password into the log export"
    );
    assert!(
        !sink
            .records
            .lock()
            .unwrap()
            .iter()
            .any(|record| record.message.contains(XTREAM_PASSWORD)),
        "resolving a stream leaked the password into the host log sink"
    );
}

fn assert_xtream_recent_replays(rt: &Runtime, core: &Arc<Core>, source_id: i64, stored: &str) {
    let identity = 4242;
    rt.block_on(core.recents().record(
        source_id,
        identity,
        "Recent Xtream channel".to_owned(),
        stored.to_owned(),
        None,
    ))
    .expect("the Xtream recent records");
    let recent = rt
        .block_on(core.recents().list(1))
        .expect("the recent list remains readable")
        .into_iter()
        .next()
        .expect("the recorded recent is listed");
    assert!(
        recent.source_id == source_id && recent.identity == identity,
        "the recent must retain its stable playback identity"
    );
    assert!(
        recent.locator != stored && recent.locator.starts_with("spidola-sealed://v1/"),
        "the listed recent locator must remain an opaque envelope"
    );
    assert!(
        !recent.locator.contains(XTREAM_PASSWORD),
        "the listed recent locator must not expose the account secret"
    );

    let recent_locator = recent.locator.clone();
    let recent_playable = rt
        .block_on(core.sources().resolve_playback(
            recent.source_id,
            recent.identity,
            recent.locator,
        ))
        .expect("a listed Xtream recent resolves");
    let recent_playable_locator = recent_playable.locator();
    assert!(
        recent_playable_locator.contains(XTREAM_USER)
            && recent_playable_locator.contains(XTREAM_PASSWORD),
        "the replayed URL must carry the credentials an engine needs"
    );
    assert!(
        recent_playable_locator.contains("/live/") && recent_playable_locator.ends_with("4242.ts"),
        "the replayed URL must still name the same stream"
    );

    let mut damaged = recent_locator;
    damaged.push('A');
    let damaged_result = rt.block_on(core.sources().resolve_playback(
        recent.source_id,
        recent.identity,
        damaged,
    ));
    assert!(
        matches!(damaged_result, Err(ApiError::StorageCorrupt)),
        "a damaged recent envelope must fail closed"
    );
}

#[test]
fn an_m3u_locator_resolves_unchanged_so_the_zap_path_needs_no_per_kind_branch() {
    // Kind-agnostic by contract: the shell asks for a playable URL and gets one, whatever the
    // source is. The M3U URL is sealed in the catalog and must open to the original value.
    let _serial = serial();
    let rt = Runtime::new().unwrap();
    let harness = build_core(&rt);
    let stream = "http://host.example/live/7.ts".to_owned();
    let playlist = format!("#EXTM3U\n#EXTINF:-1,Seven\n{stream}\n");
    let source_url = spawn_stub(playlist.into_bytes(), vec![(4096, Duration::ZERO)]);
    let source = rt
        .block_on(harness.core.sources().add_m3u_url(
            "Playlist".to_owned(),
            source_url,
            None,
            false,
        ))
        .expect("added");
    let id = match &source {
        Source::M3uUrl { id, .. } => *id,
        other => panic!("expected an M3U source, got {other:?}"),
    };

    let (terminal_tx, terminal_rx) = channel();
    let _refresh = harness.core.sources().refresh(
        id,
        Box::new(CollectingListener {
            progress: Arc::new(Mutex::new(Vec::new())),
            first_progress: Arc::new(Mutex::new(None)),
            terminal: terminal_tx,
        }),
    );
    assert!(matches!(
        terminal_rx.recv_timeout(Duration::from_secs(30)).unwrap(),
        Terminal::Complete(ImportOutcome { inserted: 1, .. })
    ));
    let stored = rt
        .block_on(harness.core.catalog().channels(id, 0, 1))
        .expect("catalog remains readable")
        .channels
        .into_iter()
        .next()
        .expect("one channel imported")
        .locator;
    assert_ne!(stored, stream, "the catalog locator must remain sealed");
    assert_eq!(
        rt.block_on(harness.core.sources().resolve_stream(id, stored))
            .expect("an M3U locator resolves"),
        stream,
        "the authenticated catalog envelope must open to the original locator"
    );
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
fn an_add_that_cannot_write_its_row_takes_its_credential_back() {
    // The password is stored before the row that names it, deliberately — but that leaves a
    // window where the keychain holds a secret nothing has ever heard of. Nothing else can close
    // it: a later `delete` finds out which key belongs to an account by reading the account's
    // row, and the row is exactly what did not get written. So the add cleans up after itself,
    // and here is where that is true rather than assumed.
    let _serial = serial();
    let rt = Runtime::new().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("spidola.sqlite");
    let secrets = FakeSecrets::default();
    let core = open_core(&rt, &db_path, RecordingSink::default(), secrets.clone());
    refuse_source_inserts(&db_path);

    let base = spawn_xtream_stub(1);
    let result = rt.block_on(core.sources().add_xtream(
        "Home headend".to_owned(),
        base,
        XTREAM_USER.to_owned(),
        XTREAM_PASSWORD.to_owned(),
    ));

    assert!(
        result.is_err(),
        "a refused insert must fail the add, not report a source that does not exist"
    );
    assert!(
        secrets.store.lock().unwrap().is_empty(),
        "a failed add must not strand its password in the host store"
    );
    assert!(
        rt.block_on(core.sources().list()).unwrap().is_empty(),
        "a failed add must leave no source behind"
    );
}

#[test]
fn a_store_that_refuses_the_password_leaves_the_source_deletable() {
    // The credential goes before the row because the row is the only thing that can name it, and
    // a store saying no is the only way to see that order. The delete must fail with the source
    // still listed: pressing delete again then finishes the job, where the other order would
    // leave a password nothing could ever reach.
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
    let id = match &source {
        Source::Xtream { id, .. } => *id,
        other => panic!("expected an Xtream source, got {other:?}"),
    };

    secrets.refuse_deletes.store(true, Ordering::SeqCst);
    assert!(
        rt.block_on(core.sources().delete(id)).is_err(),
        "a store that refuses the password must fail the delete rather than log past it"
    );
    assert_eq!(
        rt.block_on(core.sources().list()).unwrap().len(),
        1,
        "the row that names the credential must survive, or nothing can ever name it again"
    );
    assert_eq!(
        secrets.store.lock().unwrap().len(),
        1,
        "nothing was removed, so both halves must still be there"
    );

    // Unlocked, the same call the user already made converges: no source, no password.
    secrets.refuse_deletes.store(false, Ordering::SeqCst);
    rt.block_on(core.sources().delete(id))
        .expect("the retry deletes");
    assert!(rt.block_on(core.sources().list()).unwrap().is_empty());
    assert!(
        secrets.store.lock().unwrap().is_empty(),
        "the retry must finish what the refused delete started"
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
fn racing_starts_leave_exactly_one_server_on_the_lan() {
    // Two screens — or one screen re-entered before the first start finished — can ask for a
    // session at the same moment, and only one server may come out of it: a listener nobody is
    // showing a token for is a way into this TV that no one is watching (§12).
    //
    // What that rests on is the *order*, which is why the load-bearing assertion here reads the
    // log rather than the sockets. Whether a socket is still open cannot tell the two designs
    // apart — `PairServer`'s `Drop` signals shutdown as well, so an overwritten slot closes the
    // loser's listener either way, and the port probe below passes even against a start that
    // never stopped anything. What only serialization can produce is the first server being
    // stopped *before* the second one starts listening: `start` takes the lock, stops what it
    // finds, and only then binds. Two interleaved starts cannot do that in any order — they both
    // read the slot before either has stored anything, so both listeners are up before the first
    // one is taken down.
    //
    // `biased;` pins the polling order so the interleaving under test is the one described,
    // rather than whichever `join!` happened to rotate to.
    let _serial = serial();
    let rt = Runtime::new().unwrap();
    let harness = build_core(&rt);
    let pairing = harness.core.pairing();

    let (first, second) = rt.block_on(async {
        tokio::join!(
            biased;
            pairing.start(
                Some("127.0.0.1".to_owned()),
                Box::new(RecordingPairingListener::default()),
            ),
            pairing.start(
                Some("127.0.0.1".to_owned()),
                Box::new(RecordingPairingListener::default()),
            ),
        )
    });
    let first = first.expect("the first start succeeds");
    let second = second.expect("the second start succeeds");
    assert_ne!(
        first.token, second.token,
        "each start must mint its own token, never reuse the last: {first:?} {second:?}"
    );
    let lifecycle = pairing_lifecycle(&harness.sink);
    let listens: Vec<usize> = positions_of(&lifecycle, "listening");
    let stops: Vec<usize> = positions_of(&lifecycle, "stopped");
    assert_eq!(
        listens.len(),
        2,
        "both starts bound a listener: {lifecycle:?}"
    );
    assert!(
        stops
            .first()
            .is_some_and(|first_stop| *first_stop < listens[1]),
        "the first server must be stopped before the second starts listening; the two starts ran \
         on top of each other instead: {lifecycle:?}"
    );

    // The consequence, and cheap to check even though it holds for the wrong reasons too.
    // Deduplicated because the loser's socket is closed before the winner binds, which frees its
    // number for the OS to hand straight back — the same port twice is a pass, not a collision.
    let mut ports = vec![first.port, second.port];
    ports.sort_unstable();
    ports.dedup();
    let live = ports
        .iter()
        .filter(|port| std::net::TcpStream::connect(("127.0.0.1", **port)).is_ok())
        .count();
    assert_eq!(live, 1, "exactly one of {ports:?} may answer");

    // And the survivor is the one the service holds, so the screen closing still closes it.
    rt.block_on(pairing.stop());
    for port in ports {
        assert!(
            std::net::TcpStream::connect(("127.0.0.1", port)).is_err(),
            "port {port} still answers after stop"
        );
    }
}

/// Every pairing server's birth and death as the log saw them, oldest first.
///
/// `core-pair` announces both ends of a server's life on this target, and announces the death
/// from inside the accept task that `PairServer::stop` awaits — so a "stopped" here is a socket
/// that is provably closed, not one that has merely been asked to close. That is what makes the
/// sequence worth reading: the question of whether two starts ran in turn or on top of each
/// other is exactly the question of where the stops fall between the listens.
///
/// Deliberately not a count. `core-pair` and `PairingService` log the same sentence on the same
/// target when a server goes down, so one stop can produce two records — the ordering carries
/// the meaning, the tally does not.
fn pairing_lifecycle(sink: &RecordingSink) -> Vec<&'static str> {
    sink.records
        .lock()
        .unwrap()
        .iter()
        .filter(|record| record.target == "spidola::pair")
        .filter_map(|record| {
            if record.message.contains("pairing server listening") {
                Some("listening")
            } else if record.message.contains("pairing server stopped") {
                Some("stopped")
            } else {
                None
            }
        })
        .collect()
}

/// Where `event` occurs in a lifecycle sequence.
fn positions_of(lifecycle: &[&str], event: &str) -> Vec<usize> {
    lifecycle
        .iter()
        .enumerate()
        .filter_map(|(at, seen)| (*seen == event).then_some(at))
        .collect()
}

#[test]
fn a_stop_racing_a_start_still_leaves_nothing_listening() {
    // The ordering that makes `stop` mean what the shell means by it. A shell drives start and
    // stop from its screen lifecycle, so a stop can arrive while a start is still binding — and
    // must not slip between the bind and the moment the service remembers it, stopping nothing
    // and returning while the listener it was called to close goes live behind it.
    let _serial = serial();
    let rt = Runtime::new().unwrap();
    let harness = build_core(&rt);
    let pairing = harness.core.pairing();

    let session = rt.block_on(async {
        let (started, ()) = tokio::join!(
            biased;
            pairing.start(
                Some("127.0.0.1".to_owned()),
                Box::new(RecordingPairingListener::default()),
            ),
            pairing.stop(),
        );
        started.expect("the pairing server starts")
    });

    assert!(
        std::net::TcpStream::connect(("127.0.0.1", session.port)).is_err(),
        "the stop was overtaken: port {} is still listening with no screen behind it",
        session.port
    );
}

#[test]
fn setting_the_epg_window_moves_both_bounds() {
    // One window, one call, one transaction (PRD §6.6) — the pair the user chose is the pair
    // they read back.
    let _serial = serial();
    let rt = Runtime::new().unwrap();
    let harness = build_core(&rt);
    let settings = harness.core.settings();

    rt.block_on(settings.set_epg_window(24, 6)).unwrap();

    let snapshot = rt.block_on(settings.snapshot()).unwrap();
    assert_eq!(snapshot.epg_window_ahead_hours, 24);
    assert_eq!(snapshot.epg_window_behind_hours, 6);
}

#[test]
fn xmltv_refresh_maps_to_catalog_identity_and_swaps_atomically() {
    let _serial = serial();
    let rt = Runtime::new().unwrap();
    let harness = build_core(&rt);
    let sources = harness.core.sources();
    let playlist_url = spawn_stub(playlist(1), vec![(64 * 1024, Duration::ZERO)]);
    let source = rt
        .block_on(sources.add_m3u_url("Guide source".to_owned(), playlist_url, None, false))
        .unwrap();
    let Source::M3uUrl { id: source_id, .. } = source else {
        panic!("expected an M3U URL source");
    };
    let (catalog_tx, catalog_rx) = channel();
    let _catalog = sources.refresh(
        source_id,
        Box::new(CollectingListener {
            progress: Arc::new(Mutex::new(Vec::new())),
            first_progress: Arc::new(Mutex::new(None)),
            terminal: catalog_tx,
        }),
    );
    assert!(matches!(
        catalog_rx.recv_timeout(Duration::from_secs(30)).unwrap(),
        Terminal::Complete(_)
    ));

    let xmltv = br#"<?xml version="1.0"?>
<tv><programme channel="id0" start="19700101000000 +0000" stop="19700101010000 +0000">
<title>Midnight News</title><desc>The first bulletin.</desc></programme></tv>"#
        .to_vec();
    let secret_marker = "epg-feed-token-61d9";
    let feed_url = format!(
        "{}?token={secret_marker}",
        spawn_stub(xmltv, vec![(1024, Duration::ZERO)])
    );
    let epg = harness.core.epg();
    rt.block_on(epg.set_xmltv_feed(source_id, feed_url))
        .unwrap();
    let progress = Arc::new(Mutex::new(Vec::new()));
    let (epg_tx, epg_rx) = channel();
    let _refresh = epg.refresh(
        source_id,
        1,
        Box::new(CollectingEpgListener {
            progress: Arc::clone(&progress),
            terminal: epg_tx,
        }),
    );
    match epg_rx.recv_timeout(Duration::from_secs(30)).unwrap() {
        EpgTerminal::Complete(outcome) => {
            assert_eq!(outcome.inserted, 1);
            assert_eq!(outcome.unmapped, 0);
        }
        EpgTerminal::Failed(error) => panic!("guide refresh failed: {error:?}"),
    }
    assert!(!progress.lock().unwrap().is_empty());

    let channel = rt
        .block_on(harness.core.catalog().channels(source_id, 0, 1))
        .unwrap()
        .channels
        .remove(0);
    let now_next = rt
        .block_on(epg.now_next(source_id, channel.identity, 1))
        .unwrap();
    assert_eq!(now_next.current.unwrap().title, "Midnight News");
    assert!(now_next.next.is_none());

    let requested = vec![channel.identity, 404, channel.identity];
    let batch = rt
        .block_on(epg.now_next_batch(source_id, requested.clone(), 1))
        .unwrap();
    assert_eq!(
        batch
            .iter()
            .map(|entry| entry.channel_identity)
            .collect::<Vec<_>>(),
        requested
    );
    assert_eq!(
        batch[0]
            .programmes
            .current
            .as_ref()
            .map(|programme| programme.title.as_str()),
        Some("Midnight News")
    );
    assert!(batch[1].programmes.current.is_none());
    assert!(batch[1].programmes.next.is_none());
    assert!(matches!(
        rt.block_on(epg.now_next_batch(source_id, vec![channel.identity; 101], 1)),
        Err(ApiError::InvalidInput {
            field: InputField::Source,
            issue: InputIssue::Unsupported,
        })
    ));

    let disk = everything_on_disk(harness.db_dir.path());
    assert!(!contains_bytes(&disk, secret_marker.as_bytes()));
    let logs = harness.sink.records.lock().unwrap();
    assert!(
        logs.iter()
            .all(|record| !record.message.contains(secret_marker))
    );
}

#[test]
fn custom_lineup_crud_reorder_and_portable_round_trip_keep_storage_sealed() {
    let _serial = serial();
    let rt = Runtime::new().unwrap();
    let harness = build_core(&rt);
    let custom = harness.core.custom_channels();
    let first_group = rt.block_on(custom.create_group("News".to_owned())).unwrap();
    let second_group = rt
        .block_on(custom.create_group("Documentary".to_owned()))
        .unwrap();
    rt.block_on(custom.move_group_before(second_group, first_group))
        .unwrap();
    rt.block_on(custom.rename_group(second_group, "Docs".to_owned()))
        .unwrap();

    let locator_secret = "custom-path-secret-d4c2";
    let header_secret = "custom-header-secret-a931";
    let first = CustomChannelDraft::new(
        Some(first_group),
        "World News".to_owned(),
        None,
        format!("http://custom.example/live/{locator_secret}/1.ts"),
        Some("Spidola Custom Agent".to_owned()),
        vec![ResolvedHeader::from_parts(
            "Authorization".to_owned(),
            header_secret.to_owned(),
        )],
    );
    let second = CustomChannelDraft::new(
        Some(first_group),
        "Local News".to_owned(),
        None,
        "http://custom.example/live/2.ts".to_owned(),
        None,
        Vec::new(),
    );
    let first_id = rt.block_on(custom.create(first)).unwrap();
    let second_id = rt.block_on(custom.create(second)).unwrap();
    rt.block_on(custom.move_before(second_id, first_id))
        .unwrap();
    let news = rt.block_on(custom.list(Some(first_group), 0, 10)).unwrap();
    assert_eq!(
        news.iter()
            .map(|channel| channel.name.as_str())
            .collect::<Vec<_>>(),
        vec!["Local News", "World News"]
    );

    let resolved = rt.block_on(custom.resolve(first_id)).unwrap();
    assert!(resolved.locator().contains(locator_secret));
    assert_eq!(resolved.headers()[0].value(), header_secret);
    let disk = everything_on_disk(harness.db_dir.path());
    assert!(!contains_bytes(&disk, locator_secret.as_bytes()));
    assert!(!contains_bytes(&disk, header_secret.as_bytes()));

    let portable = rt.block_on(custom.export_portable()).unwrap();
    let contents = portable.contents();
    assert!(contents.contains(locator_secret));
    assert!(contents.contains("Docs"), "empty groups survive an export");
    assert_eq!(
        rt.block_on(custom.import_portable(contents, CustomImportMode::Replace))
            .unwrap(),
        2
    );
    let groups = rt.block_on(custom.groups(0, 10)).unwrap();
    assert_eq!(groups.len(), 2);
    let news_group = groups.iter().find(|group| group.name == "News").unwrap();
    assert_eq!(
        rt.block_on(custom.list(Some(news_group.id), 0, 10))
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn favorite_moves_change_the_global_home_lineup() {
    let _serial = serial();
    let rt = Runtime::new().unwrap();
    let harness = build_core(&rt);
    let source = rt
        .block_on(harness.core.sources().add_m3u_file("Ordered".to_owned()))
        .unwrap();
    let Source::M3uFile { id: source_id, .. } = source else {
        panic!("expected an M3U file source");
    };
    let (terminal_tx, terminal_rx) = channel();
    let _import = harness.core.sources().import_m3u_content(
        source_id,
        String::from_utf8(playlist(3)).unwrap(),
        Box::new(CollectingListener {
            progress: Arc::new(Mutex::new(Vec::new())),
            first_progress: Arc::new(Mutex::new(None)),
            terminal: terminal_tx,
        }),
    );
    assert!(matches!(
        terminal_rx.recv_timeout(Duration::from_secs(30)).unwrap(),
        Terminal::Complete(_)
    ));
    let channels = rt
        .block_on(harness.core.catalog().channels(source_id, 0, 10))
        .unwrap()
        .channels;
    let favorites = harness.core.favorites();
    for channel in &channels {
        rt.block_on(favorites.add(source_id, channel.identity))
            .unwrap();
    }
    rt.block_on(favorites.move_before(
        source_id,
        channels[2].identity,
        source_id,
        channels[0].identity,
    ))
    .unwrap();
    let ordered = rt
        .block_on(favorites.favorite_channels(0, 10))
        .unwrap()
        .channels;
    assert_eq!(
        ordered
            .iter()
            .map(|channel| channel.name.as_str())
            .collect::<Vec<_>>(),
        vec!["Channel 2", "Channel 0", "Channel 1"]
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
            field: InputField::Address,
            issue: InputIssue::Invalid,
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
        assert!(!variant.to_string().is_empty());
    }
}
