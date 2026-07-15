// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! `core-api` — the façade composing the other crates and the composition root; the only
//! crate the apps see (TECH_SPEC §4.6, §5).
//!
//! [`Core`] is constructed once at startup with the host's [`secrets`] and log-sink callbacks,
//! owns the Tokio [`runtime`] (invisible to the shells) and the SQLite database, and vends the
//! UniFFI service objects. Long operations return a [`events::TaskHandle`] and report progress
//! through a listener; read paths are paged by contract; errors flatten into the stable
//! [`error`] taxonomy; and every event streams through the [`logging`] pipeline into the host
//! sink. The [`Handshake`] lets a shell verify core/schema/boundary versions before use
//! (§13).
//!
//! Note: unlike the other crates this one does NOT `#![forbid(unsafe_code)]`, because it hosts
//! UniFFI-generated FFI glue; `unsafe` is warned at the workspace level.

pub mod error;
pub mod events;
pub mod import;
pub mod logging;
pub mod records;
pub mod runtime;
pub mod secrets;
pub mod services;
pub mod settings;
pub mod xtream;

use std::path::Path;
use std::sync::Arc;

use core_db::Db;

pub use error::{ApiError, ErrorUx, UserAction};
pub use events::{ImportListener, ImportOutcome, ImportProgress, ImportStage, TaskHandle};
pub use logging::{LogConfig, LogHandle, LogLevel, LogRecord, LogSink, RingBuffer, RingLayer};
pub use records::{
    BrowseGroup, BrowseGroupPage, Channel, ChannelOverrides, ChannelPage, Favorite, HeaderField,
    MediaKind, Recent, SearchPage, Source, SourceCommon, SourceKind,
};
pub use runtime::CoreRuntime;
pub use secrets::SecretStore;
pub use services::{
    CatalogService, FavoritesService, PairingListener, PairingService, PairingSession,
    PairingSubmission, RecentsService, SearchService, SettingsService, SourceService,
};
pub use settings::{
    AppSettings, BufferingProfile, InterfaceDensity, SubtitleBackground, SubtitleSize,
};

use crate::logging::targets;

uniffi::setup_scaffolding!();

/// The FFI boundary version. Bumped whenever the exported surface changes shape; the startup
/// [`Handshake`] lets an older shell refuse a newer core legibly rather than crash (TECH_SPEC
/// §5, §13).
///
/// `2` — Phase 6: added the Xtream and pairing services, and replaced `SettingsService`'s
/// opaque key/value methods with the typed [`settings`] surface.
pub const BOUNDARY_VERSION: u32 = 2;

/// The core's build-time git revision, for the diagnostics screen (PRD §6.9). Resolved by
/// `build.rs`; `"unknown"` in a source tree without git metadata (a release tarball build),
/// which is reported honestly rather than failing the build.
pub const GIT_REVISION: &str = env!("SPIDOLA_GIT_REVISION");

/// Startup configuration handed to [`Core::new`].
#[derive(Debug, Clone, uniffi::Record)]
pub struct CoreConfig {
    /// Filesystem path to the SQLite database file (created if absent, migrated to head).
    pub db_path: String,
    /// Initial `tracing` `EnvFilter` directives (e.g. `"info"`, `"spidola::db=debug,info"`).
    pub log_directives: String,
}

/// The versions a shell checks at startup before trusting the boundary (TECH_SPEC §13).
///
/// A downgraded shell that finds a `schema_version` newer than it understands must refuse to
/// proceed with a clear message rather than guess, since migrations are forward-only.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Handshake {
    /// The core crate's semantic version.
    pub core_version: String,
    /// The core's build-time git revision ([`GIT_REVISION`]), shown on the diagnostics screen so
    /// a support thread can name the exact core build (PRD §6.9).
    pub core_git_revision: String,
    /// The database schema version at head.
    pub schema_version: u32,
    /// The FFI boundary version ([`BOUNDARY_VERSION`]).
    pub boundary_version: u32,
}

/// The composition root: owns the runtime and database, and vends the service objects.
#[derive(uniffi::Object)]
pub struct Core {
    rt: Arc<CoreRuntime>,
    db: Arc<Db>,
    log: LogHandle,
    // One shared `SourceService` for the process, so its in-flight-refresh registry is the same
    // instance every `sources()` call hands out. A best-effort `delete` can then find and cancel a
    // sibling `refresh`'s token instead of consulting an empty registry on a throwaway instance.
    source_service: Arc<SourceService>,
    // One shared `PairingService` for the same reason, and a stronger one: it *holds* the running
    // server, so a throwaway instance per `pairing()` call would drop the listener the moment the
    // shell released the handle it started from.
    pairing_service: Arc<PairingService>,
}

#[uniffi::export]
impl Core {
    /// Initializes the core: installs the logging pipeline and host sink, builds the owned
    /// runtime, and opens (creating and migrating) the database.
    ///
    /// # Errors
    /// Returns [`ApiError::Internal`] if the logging pipeline cannot be installed (a competing
    /// `tracing` subscriber already exists, or a live `Core` already owns the host-sink slot — a
    /// `Core` is single-per-process), or the runtime cannot start, and [`ApiError::StorageCorrupt`]
    /// if the database cannot be opened or migrated.
    #[uniffi::constructor]
    pub fn new(
        config: CoreConfig,
        secrets: Box<dyn SecretStore>,
        log_sink: Box<dyn LogSink>,
    ) -> Result<Arc<Self>, ApiError> {
        let log = logging::init(&LogConfig {
            default_directives: config.log_directives.clone(),
            ring_capacity: logging::DEFAULT_RING_CAPACITY,
        })
        .map_err(|_| ApiError::Internal)?;

        let rt = Arc::new(CoreRuntime::new()?);
        let db = Arc::new(Db::open(Path::new(&config.db_path)).map_err(ApiError::from)?);
        // Establish this `Core`'s filter explicitly, and unconditionally. A log level the user
        // chose on the diagnostics screen outranks the start-up directives, so their choice
        // survives a restart (PRD §6.9); with no stored level the caller's directives stand.
        // The `else` branch is not redundant with `init`'s `default_directives`: `init` computes
        // the pipeline once per process, so a second `Core` in one process would otherwise
        // silently inherit the *previous* `Core`'s filter instead of honouring its own config.
        // Applying it before the host sink is installed means the first records already comply.
        let directives = match SettingsService::stored_log_level(&db)? {
            Some(level) => level.directive().to_owned(),
            None => config.log_directives,
        };
        if log.set_directives(&directives).is_err() {
            tracing::warn!(
                target: targets::DB,
                "log directives could not be applied; the pipeline keeps its previous filter"
            );
        }
        let secrets: Arc<dyn SecretStore> = Arc::from(secrets);
        let source_service = SourceService::new(Arc::clone(&rt), Arc::clone(&db), secrets);
        let pairing_service = PairingService::new(Arc::clone(&rt));
        // Claim the process-global host-sink slot last: it fails if a live `Core` already owns it,
        // and installing only after the fallible steps above means a transient runtime/db failure
        // never leaves the slot occupied (which would then block every future construction).
        logging::install_sink(Arc::from(log_sink)).map_err(|_| ApiError::Internal)?;
        tracing::info!(target: targets::DB, path = %config.db_path, "core initialized");
        Ok(Arc::new(Self {
            rt,
            db,
            log,
            source_service,
            pairing_service,
        }))
    }

    /// The startup handshake: core, schema, and boundary versions.
    #[must_use]
    pub fn handshake(&self) -> Handshake {
        Handshake {
            core_version: env!("CARGO_PKG_VERSION").to_owned(),
            core_git_revision: GIT_REVISION.to_owned(),
            schema_version: u32::try_from(core_db::SCHEMA_VERSION).unwrap_or(u32::MAX),
            boundary_version: BOUNDARY_VERSION,
        }
    }

    /// The source service (add / list / refresh / rename / disable / delete).
    ///
    /// Returns the one shared instance, so its in-flight-refresh registry is consistent across
    /// calls and a `delete` can cancel a concurrent `refresh` on the same source.
    #[must_use]
    pub fn sources(&self) -> Arc<SourceService> {
        Arc::clone(&self.source_service)
    }

    /// The catalog service (paged browse queries).
    #[must_use]
    pub fn catalog(&self) -> Arc<CatalogService> {
        CatalogService::new(Arc::clone(&self.rt), Arc::clone(&self.db))
    }

    /// The search service (ranked, paged results).
    #[must_use]
    pub fn search(&self) -> Arc<SearchService> {
        SearchService::new(Arc::clone(&self.rt), Arc::clone(&self.db))
    }

    /// The favorites service.
    #[must_use]
    pub fn favorites(&self) -> Arc<FavoritesService> {
        FavoritesService::new(Arc::clone(&self.rt), Arc::clone(&self.db))
    }

    /// The recently-watched service (list, purge, off-switch).
    #[must_use]
    pub fn recents(&self) -> Arc<RecentsService> {
        RecentsService::new(Arc::clone(&self.rt), Arc::clone(&self.db))
    }

    /// The pairing service (start/stop the LAN server, PRD §6.1).
    ///
    /// Returns the one shared instance, which owns the running server — a per-call instance
    /// would close the socket as soon as the caller dropped its handle.
    #[must_use]
    pub fn pairing(&self) -> Arc<PairingService> {
        Arc::clone(&self.pairing_service)
    }

    /// The settings service (typed surface with defaults, PRD §6.9).
    #[must_use]
    pub fn settings(&self) -> Arc<SettingsService> {
        SettingsService::new(Arc::clone(&self.rt), Arc::clone(&self.db), self.log.clone())
    }

    /// Reloads the runtime log level from `EnvFilter` directives (diagnostics screen, §4.8).
    ///
    /// # Errors
    /// Returns [`ApiError::InvalidInput`] if the directives cannot be parsed.
    // UniFFI lifts the foreign string into an owned `String`; we only need to borrow it.
    #[allow(clippy::needless_pass_by_value)]
    pub fn set_log_level(&self, directives: String) -> Result<(), ApiError> {
        self.log
            .set_directives(&directives)
            .map_err(|_| ApiError::InvalidInput {
                reason: "that log level isn't valid".to_owned(),
            })
    }

    /// Snapshots the recent log lines for the diagnostics log-export (redaction proven in
    /// `logging`).
    #[must_use]
    pub fn export_logs(&self) -> Vec<String> {
        self.log.export_logs()
    }
}

impl Drop for Core {
    /// Releases the process-global host-sink slot this `Core` claimed in [`Core::new`], so its
    /// callback into a now-torn-down host is not retained and a subsequently constructed `Core`
    /// installs its own sink cleanly. Sound because [`Core::new`] refuses a second live `Core`,
    /// so the slot this drop clears is always the one this `Core` installed.
    fn drop(&mut self) {
        logging::clear_sink();
    }
}
