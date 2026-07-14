// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! `SettingsService` (TECH_SPEC §4.6, PRD §6.9): the typed settings surface over `core-db`'s
//! flat key→value store.
//!
//! The vocabulary — keys, closed value sets, and code defaults — lives in [`crate::settings`];
//! this is the service that speaks it. The boundary is **typed by contract**: no key string
//! and no free-form value crosses the FFI, so a shell cannot invent a setting outside the
//! vocabulary. (The opaque get/set/all surface this service carried through Phase 2 is gone,
//! exactly as that phase's note anticipated.) Reads resolve through the defaults, so a fresh
//! install answers every question without a single stored row — PRD §6.9's "fully usable
//! without ever opening settings", enforced in the core rather than re-implemented per shell.
//!
//! Two settings have owners elsewhere and are deliberately not duplicated here.
//! [`Self::snapshot`] *reports* the recently-watched off-switch, but
//! [`RecentsService::set_enabled`](crate::services::RecentsService::set_enabled) stays its only
//! writer — that service enforces the switch, so it owns it. The log level is the mirror case:
//! this service owns it, and writing it both persists the choice *and* applies it to the live
//! `tracing` filter in one call, so the stored value and the running filter cannot disagree.
//!
//! No secret ever lands here (TECH_SPEC §12).

use std::collections::HashMap;
use std::sync::Arc;

use core_db::{Db, repo};

use crate::error::ApiError;
use crate::logging::{LogHandle, LogLevel};
use crate::runtime::CoreRuntime;
use crate::settings::{
    AppSettings, BufferingProfile, InterfaceDensity, StoredValue, SubtitleBackground, SubtitleSize,
    count_from, keys, recents_enabled_from,
};

/// Reads and writes persisted settings, resolved through their defaults.
#[derive(uniffi::Object)]
pub struct SettingsService {
    rt: Arc<CoreRuntime>,
    db: Arc<Db>,
    log: LogHandle,
}

impl SettingsService {
    /// Builds the service over shared runtime, database, and logging handles.
    pub(crate) fn new(rt: Arc<CoreRuntime>, db: Arc<Db>, log: LogHandle) -> Arc<Self> {
        Arc::new(Self { rt, db, log })
    }

    /// The persisted log level, or `None` if the user never chose one, so [`crate::Core::new`]
    /// can apply their choice at startup rather than waiting for the settings screen to open.
    ///
    /// Deliberately *not* default-resolving, unlike every read in [`Self::snapshot`]: "unset"
    /// and "set to Info" must stay distinguishable here, because an unset level leaves the
    /// caller's start-up directives in force (which is how a dev build keeps its own, richer
    /// per-target filter) while a stored one overrides them.
    ///
    /// # Errors
    /// Returns [`ApiError::StorageCorrupt`] on a query failure.
    pub(crate) fn stored_log_level(db: &Db) -> Result<Option<LogLevel>, ApiError> {
        let conn = db.reader()?;
        Ok(repo::settings::get(&conn, keys::LOG_LEVEL)?
            .as_deref()
            .and_then(LogLevel::parse_stored))
    }

    /// Writes one vocabulary key on the blocking pool.
    async fn put(&self, key: String, value: String) -> Result<(), ApiError> {
        let db = Arc::clone(&self.db);
        self.rt
            .run_blocking(move || {
                let conn = db.writer();
                repo::settings::set(&conn, &key, &value)?;
                Ok(())
            })
            .await
    }

    /// Clears one vocabulary key, reverting it to its code default.
    async fn clear(&self, key: String) -> Result<(), ApiError> {
        let db = Arc::clone(&self.db);
        self.rt
            .run_blocking(move || {
                let conn = db.writer();
                repo::settings::remove(&conn, &key)?;
                Ok(())
            })
            .await
    }

    /// Writes `value` under `key`, or clears the key when `value` is `None` — the shape every
    /// "override, else fall back to the default" setting needs (the engine keys, the language).
    /// Clearing rather than storing a sentinel keeps "unset" and "set to the default value"
    /// the same state, so a changed default reaches users who never touched the setting.
    async fn put_optional(&self, key: String, value: Option<String>) -> Result<(), ApiError> {
        match value {
            Some(value) => self.put(key, value).await,
            None => self.clear(key).await,
        }
    }
}

#[uniffi::export]
impl SettingsService {
    /// Every setting resolved to a value: stored where the user set one, code default
    /// otherwise. One call, because the settings screen wants all of them at once.
    ///
    /// # Errors
    /// Returns [`ApiError::StorageCorrupt`] on a query failure.
    pub async fn snapshot(&self) -> Result<AppSettings, ApiError> {
        let db = Arc::clone(&self.db);
        self.rt
            .run_blocking(move || {
                let conn = db.reader()?;
                // One read of the whole (small) table beats a dozen point queries, and every
                // value in a snapshot then comes from the same instant.
                let stored: HashMap<String, String> =
                    repo::settings::all(&conn)?.into_iter().collect();
                let get = |key: &str| stored.get(key).map(String::as_str);
                let defaults = AppSettings::default();
                Ok(AppSettings {
                    default_engine: get(keys::DEFAULT_ENGINE).map(str::to_owned),
                    buffering: BufferingProfile::from_stored(get(keys::BUFFERING)),
                    subtitle_size: SubtitleSize::from_stored(get(keys::SUBTITLE_SIZE)),
                    subtitle_background: SubtitleBackground::from_stored(get(
                        keys::SUBTITLE_BACKGROUND,
                    )),
                    language: get(keys::LANGUAGE).map(str::to_owned),
                    density: InterfaceDensity::from_stored(get(keys::DENSITY)),
                    recents_enabled: recents_enabled_from(get(keys::RECENTS_ENABLED)),
                    recents_retention_days: count_from(
                        get(keys::RECENTS_RETENTION_DAYS),
                        defaults.recents_retention_days,
                    ),
                    epg_window_ahead_hours: count_from(
                        get(keys::EPG_WINDOW_AHEAD_HOURS),
                        defaults.epg_window_ahead_hours,
                    ),
                    epg_window_behind_hours: count_from(
                        get(keys::EPG_WINDOW_BEHIND_HOURS),
                        defaults.epg_window_behind_hours,
                    ),
                    image_cache_max_mb: count_from(
                        get(keys::IMAGE_CACHE_MAX_MB),
                        defaults.image_cache_max_mb,
                    ),
                    log_level: LogLevel::from_stored(get(keys::LOG_LEVEL)),
                })
            })
            .await
    }

    /// Sets the global default engine, or clears it with `None` to fall back to the platform
    /// default. The key is opaque to the core — the shell's selection policy resolves it
    /// (TECH_SPEC §8), so the core stores the choice without holding an opinion on it.
    ///
    /// # Errors
    /// Returns [`ApiError::StorageCorrupt`] on a write failure.
    pub async fn set_default_engine(&self, engine: Option<String>) -> Result<(), ApiError> {
        self.put_optional(keys::DEFAULT_ENGINE.to_owned(), engine)
            .await
    }

    /// The per-source engine override, if the user set one for this source.
    ///
    /// # Errors
    /// Returns [`ApiError::StorageCorrupt`] on a query failure.
    pub async fn engine_for_source(&self, source_id: i64) -> Result<Option<String>, ApiError> {
        let db = Arc::clone(&self.db);
        self.rt
            .run_blocking(move || {
                let conn = db.reader()?;
                Ok(repo::settings::get(&conn, &keys::source_engine(source_id))?)
            })
            .await
    }

    /// Sets a per-source engine override, or clears it with `None` (the PRD §6.3 selection
    /// policy's middle tier: channel → source → platform default).
    ///
    /// # Errors
    /// Returns [`ApiError::StorageCorrupt`] on a write failure.
    pub async fn set_engine_for_source(
        &self,
        source_id: i64,
        engine: Option<String>,
    ) -> Result<(), ApiError> {
        self.put_optional(keys::source_engine(source_id), engine)
            .await
    }

    /// The per-channel engine override, if the user chose "remember for this channel" after a
    /// loud fallback (PRD §6.3) — the **top** tier of the selection policy.
    ///
    /// `identity` is the channel's stable identity hash, not its rowid: the override has to
    /// outlive a refresh, and refresh replaces every row (TECH_SPEC §4.4).
    ///
    /// # Errors
    /// Returns [`ApiError::StorageCorrupt`] on a query failure.
    pub async fn engine_for_channel(
        &self,
        source_id: i64,
        identity: i64,
    ) -> Result<Option<String>, ApiError> {
        let db = Arc::clone(&self.db);
        self.rt
            .run_blocking(move || {
                let conn = db.reader()?;
                Ok(repo::settings::get(
                    &conn,
                    &keys::channel_engine(source_id, identity),
                )?)
            })
            .await
    }

    /// Sets a per-channel engine override, or clears it with `None`.
    ///
    /// This is what the loud fallback's "remember for this channel" toggle writes: only
    /// `UnsupportedFormat`/`DecoderFailed` offer it, and only the user's press stores it —
    /// automatic switching is never silent (TECH_SPEC §14).
    ///
    /// # Errors
    /// Returns [`ApiError::StorageCorrupt`] on a write failure.
    pub async fn set_engine_for_channel(
        &self,
        source_id: i64,
        identity: i64,
        engine: Option<String>,
    ) -> Result<(), ApiError> {
        self.put_optional(keys::channel_engine(source_id, identity), engine)
            .await
    }

    /// Sets the buffering profile.
    ///
    /// # Errors
    /// Returns [`ApiError::StorageCorrupt`] on a write failure.
    pub async fn set_buffering(&self, profile: BufferingProfile) -> Result<(), ApiError> {
        self.put(keys::BUFFERING.to_owned(), profile.as_stored().to_owned())
            .await
    }

    /// Sets the subtitle glyph size.
    ///
    /// # Errors
    /// Returns [`ApiError::StorageCorrupt`] on a write failure.
    pub async fn set_subtitle_size(&self, size: SubtitleSize) -> Result<(), ApiError> {
        self.put(keys::SUBTITLE_SIZE.to_owned(), size.as_stored().to_owned())
            .await
    }

    /// Sets the subtitle backing treatment.
    ///
    /// # Errors
    /// Returns [`ApiError::StorageCorrupt`] on a write failure.
    pub async fn set_subtitle_background(
        &self,
        background: SubtitleBackground,
    ) -> Result<(), ApiError> {
        self.put(
            keys::SUBTITLE_BACKGROUND.to_owned(),
            background.as_stored().to_owned(),
        )
        .await
    }

    /// Sets the UI language to a BCP-47 tag, or clears it with `None` to follow the system.
    ///
    /// # Errors
    /// Returns [`ApiError::StorageCorrupt`] on a write failure.
    pub async fn set_language(&self, tag: Option<String>) -> Result<(), ApiError> {
        self.put_optional(keys::LANGUAGE.to_owned(), tag).await
    }

    /// Sets the interface density.
    ///
    /// # Errors
    /// Returns [`ApiError::StorageCorrupt`] on a write failure.
    pub async fn set_density(&self, density: InterfaceDensity) -> Result<(), ApiError> {
        self.put(keys::DENSITY.to_owned(), density.as_stored().to_owned())
            .await
    }

    /// Sets how many days of recently-watched history to keep.
    ///
    /// # Errors
    /// Returns [`ApiError::StorageCorrupt`] on a write failure.
    pub async fn set_recents_retention_days(&self, days: u32) -> Result<(), ApiError> {
        self.put(keys::RECENTS_RETENTION_DAYS.to_owned(), days.to_string())
            .await
    }

    /// Sets the EPG rolling window (PRD §6.6). Both bounds move together because they describe
    /// one window; separate setters would invite a half-applied intermediate state.
    ///
    /// # Errors
    /// Returns [`ApiError::StorageCorrupt`] on a write failure.
    pub async fn set_epg_window(
        &self,
        ahead_hours: u32,
        behind_hours: u32,
    ) -> Result<(), ApiError> {
        self.put(
            keys::EPG_WINDOW_AHEAD_HOURS.to_owned(),
            ahead_hours.to_string(),
        )
        .await?;
        self.put(
            keys::EPG_WINDOW_BEHIND_HOURS.to_owned(),
            behind_hours.to_string(),
        )
        .await
    }

    /// Sets the image disk-cache ceiling in megabytes. The core persists it; each shell's
    /// artwork pipeline reads it and sizes its own cache — images are the one thing a shell
    /// may cache durably (TECH_SPEC §4.5).
    ///
    /// # Errors
    /// Returns [`ApiError::StorageCorrupt`] on a write failure.
    pub async fn set_image_cache_max_mb(&self, megabytes: u32) -> Result<(), ApiError> {
        self.put(keys::IMAGE_CACHE_MAX_MB.to_owned(), megabytes.to_string())
            .await
    }

    /// Sets the diagnostics log level: persists the choice **and** applies it to the live
    /// `tracing` filter, so it survives a restart and takes effect without one (PRD §6.9,
    /// TECH_SPEC §4.8). Disabled levels cost nothing once the filter reloads.
    ///
    /// # Errors
    /// Returns [`ApiError::StorageCorrupt`] on a write failure, or [`ApiError::Internal`] if
    /// the filter reload fails — which for a [`LogLevel`] means an internal inconsistency, not
    /// bad input, since the directive comes from a closed set rather than user text.
    pub async fn set_log_level(&self, level: LogLevel) -> Result<(), ApiError> {
        self.put(keys::LOG_LEVEL.to_owned(), level.as_stored().to_owned())
            .await?;
        self.log
            .set_directives(level.directive())
            .map_err(|_| ApiError::Internal)
    }
}
