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

use std::path::Path;
use std::sync::Arc;

use core_db::Db;

pub use error::{ApiError, ErrorUx, UserAction};
pub use events::{ImportListener, ImportOutcome, ImportProgress, ImportStage, TaskHandle};
pub use logging::{LogConfig, LogHandle, LogLevel, LogRecord, LogSink, RingBuffer, RingLayer};
pub use records::{
    Channel, ChannelOverrides, ChannelPage, Favorite, HeaderField, MediaKind, SearchPage,
    SettingEntry, Source, SourceCommon, SourceKind,
};
pub use runtime::CoreRuntime;
pub use secrets::SecretStore;
pub use services::{
    CatalogService, FavoritesService, SearchService, SettingsService, SourceService,
};

use crate::logging::targets;

uniffi::setup_scaffolding!();

/// The FFI boundary version. Bumped whenever the exported surface changes shape; the startup
/// [`Handshake`] lets an older shell refuse a newer core legibly rather than crash (TECH_SPEC
/// §5, §13).
pub const BOUNDARY_VERSION: u32 = 1;

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
    // Installed now so the host-secrets boundary and its threading contract are exercised in
    // Phase 2; the first consumer (Xtream auth / authed-artwork resolver) lands in Phase 6.
    #[allow(dead_code)]
    secrets: Arc<dyn SecretStore>,
}

#[uniffi::export]
impl Core {
    /// Initializes the core: installs the logging pipeline and host sink, builds the owned
    /// runtime, and opens (creating and migrating) the database.
    ///
    /// # Errors
    /// Returns [`ApiError::Internal`] if the logging pipeline cannot be installed (a competing
    /// `tracing` subscriber already exists) or the runtime cannot start, and
    /// [`ApiError::StorageCorrupt`] if the database cannot be opened or migrated.
    #[uniffi::constructor]
    pub fn new(
        config: CoreConfig,
        secrets: Box<dyn SecretStore>,
        log_sink: Box<dyn LogSink>,
    ) -> Result<Arc<Self>, ApiError> {
        let log = logging::init(&LogConfig {
            default_directives: config.log_directives,
            ring_capacity: logging::DEFAULT_RING_CAPACITY,
        })
        .map_err(|_| ApiError::Internal)?;
        logging::install_sink(Arc::from(log_sink));

        let rt = Arc::new(CoreRuntime::new()?);
        let db = Db::open(Path::new(&config.db_path)).map_err(ApiError::from)?;
        tracing::info!(target: targets::DB, path = %config.db_path, "core initialized");
        Ok(Arc::new(Self {
            rt,
            db: Arc::new(db),
            log,
            secrets: Arc::from(secrets),
        }))
    }

    /// The startup handshake: core, schema, and boundary versions.
    #[must_use]
    pub fn handshake(&self) -> Handshake {
        Handshake {
            core_version: env!("CARGO_PKG_VERSION").to_owned(),
            schema_version: u32::try_from(core_db::SCHEMA_VERSION).unwrap_or(u32::MAX),
            boundary_version: BOUNDARY_VERSION,
        }
    }

    /// The source service (add / list / refresh / rename / disable / delete).
    #[must_use]
    pub fn sources(&self) -> Arc<SourceService> {
        SourceService::new(Arc::clone(&self.rt), Arc::clone(&self.db))
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

    /// The settings service.
    #[must_use]
    pub fn settings(&self) -> Arc<SettingsService> {
        SettingsService::new(Arc::clone(&self.rt), Arc::clone(&self.db))
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
