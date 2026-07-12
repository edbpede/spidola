// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! `SettingsService` (TECH_SPEC §4.6, PRD §6.9). A flat opaque key/value store; the typed
//! settings surface and defaults land in Phase 6. No secret ever lands here (§12).

use std::sync::Arc;

use core_db::{Db, repo};

use crate::error::ApiError;
use crate::records::SettingEntry;
use crate::runtime::CoreRuntime;

/// Reads and writes persisted settings.
#[derive(uniffi::Object)]
pub struct SettingsService {
    rt: Arc<CoreRuntime>,
    db: Arc<Db>,
}

impl SettingsService {
    /// Builds the service over shared runtime and database handles.
    pub(crate) fn new(rt: Arc<CoreRuntime>, db: Arc<Db>) -> Arc<Self> {
        Arc::new(Self { rt, db })
    }
}

#[uniffi::export]
impl SettingsService {
    /// Reads a setting value, if present.
    ///
    /// # Errors
    /// Returns [`ApiError::StorageCorrupt`] on a query failure.
    pub async fn get(&self, key: String) -> Result<Option<String>, ApiError> {
        let db = Arc::clone(&self.db);
        self.rt
            .run_blocking(move || {
                let conn = db.reader()?;
                Ok(repo::settings::get(&conn, &key)?)
            })
            .await
    }

    /// Writes (upserts) a setting value.
    ///
    /// # Errors
    /// Returns [`ApiError::StorageCorrupt`] on a write failure.
    pub async fn set(&self, key: String, value: String) -> Result<(), ApiError> {
        let db = Arc::clone(&self.db);
        self.rt
            .run_blocking(move || {
                let conn = db.writer();
                repo::settings::set(&conn, &key, &value)?;
                Ok(())
            })
            .await
    }

    /// Removes a setting, reverting it to its code default.
    ///
    /// # Errors
    /// Returns [`ApiError::StorageCorrupt`] on a write failure.
    pub async fn remove(&self, key: String) -> Result<(), ApiError> {
        let db = Arc::clone(&self.db);
        self.rt
            .run_blocking(move || {
                let conn = db.writer();
                repo::settings::remove(&conn, &key)?;
                Ok(())
            })
            .await
    }

    /// Returns every stored setting as key/value pairs.
    ///
    /// # Errors
    /// Returns [`ApiError::StorageCorrupt`] on a query failure.
    pub async fn all(&self) -> Result<Vec<SettingEntry>, ApiError> {
        let db = Arc::clone(&self.db);
        self.rt
            .run_blocking(move || {
                let conn = db.reader()?;
                let entries = repo::settings::all(&conn)?;
                Ok(entries
                    .into_iter()
                    .map(|(key, value)| SettingEntry { key, value })
                    .collect())
            })
            .await
    }
}
