// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! `RecentsService` (TECH_SPEC §4.6, PRD §6.5): the "recently watched" list with its purge
//! toggle and off-switch. Entries snapshot the name + locator at play time and key on the
//! stable channel identity, so a recent stays replayable across refreshes. The list never
//! leaves the device.
//!
//! The off-switch (a household that wants no history at all) is enforced **in the core**, at
//! [`RecentsService::record`]: when recording is off the record is a no-op, so no shell can
//! bypass the setting. `core-db` has no clock; the play timestamp is injected here.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use core_db::{Db, NewHistory, repo};
use core_model::ids::SourceId;
use core_model::locator::StreamLocator;

use crate::error::ApiError;
use crate::records::{Recent, identity_from_storage};
use crate::runtime::CoreRuntime;

/// Settings key backing the off-switch. Absent (the default) or any value but `"0"` means
/// recording is on; `"0"` means off.
const ENABLED_KEY: &str = "recents.enabled";

/// Interprets the stored off-switch value: recording is on unless it is explicitly `"0"`, so a
/// fresh install records by default (PRD §6.5 "usable without opening settings").
fn enabled_from(stored: Option<&str>) -> bool {
    stored != Some("0")
}

/// Manages the recently-watched list.
#[derive(uniffi::Object)]
pub struct RecentsService {
    rt: Arc<CoreRuntime>,
    db: Arc<Db>,
}

impl RecentsService {
    /// Builds the service over shared runtime and database handles.
    pub(crate) fn new(rt: Arc<CoreRuntime>, db: Arc<Db>) -> Arc<Self> {
        Arc::new(Self { rt, db })
    }
}

#[uniffi::export]
impl RecentsService {
    /// Records a playback event (invoked by the shell when a channel is opened, and by the
    /// player on progress in Phase 5). A no-op when the off-switch is set, so the setting is
    /// authoritative in the core.
    ///
    /// # Errors
    /// Returns [`ApiError::InvalidInput`] if `locator` is not a valid stream address, or
    /// [`ApiError::StorageCorrupt`] on a write failure.
    // UniFFI lifts the foreign strings into owned `String`s.
    #[allow(clippy::needless_pass_by_value)]
    pub async fn record(
        &self,
        source_id: i64,
        identity: i64,
        name: String,
        locator: String,
        position_secs: Option<u32>,
    ) -> Result<(), ApiError> {
        let db = Arc::clone(&self.db);
        let played_at = now_unix();
        self.rt
            .run_blocking(move || {
                let conn = db.writer();
                if !enabled_from(repo::settings::get(&conn, ENABLED_KEY)?.as_deref()) {
                    return Ok(()); // off-switch: silently keep nothing
                }
                let entry = NewHistory {
                    source_id: SourceId::new(source_id),
                    identity: identity_from_storage(identity),
                    name,
                    locator: StreamLocator::parse(&locator)?, // parse, don't validate
                    played_at_unix: played_at,
                    position_secs,
                };
                repo::history::record(&conn, &entry)?;
                Ok(())
            })
            .await
    }

    /// Returns the most recently watched entries, newest first, capped at `limit`.
    ///
    /// # Errors
    /// Returns [`ApiError::StorageCorrupt`] on a query failure.
    pub async fn list(&self, limit: u32) -> Result<Vec<Recent>, ApiError> {
        let db = Arc::clone(&self.db);
        self.rt
            .run_blocking(move || {
                let conn = db.reader()?;
                let entries = repo::history::recent(&conn, limit)?;
                Ok(entries.into_iter().map(Recent::from).collect())
            })
            .await
    }

    /// Purges the entire recently-watched list (the one-toggle purge, PRD §6.5).
    ///
    /// # Errors
    /// Returns [`ApiError::StorageCorrupt`] on a write failure.
    pub async fn clear(&self) -> Result<(), ApiError> {
        let db = Arc::clone(&self.db);
        self.rt
            .run_blocking(move || {
                let conn = db.writer();
                repo::history::clear(&conn)?;
                Ok(())
            })
            .await
    }

    /// Whether recording is on (the off-switch, PRD §6.5). Defaults to on.
    ///
    /// # Errors
    /// Returns [`ApiError::StorageCorrupt`] on a query failure.
    pub async fn is_enabled(&self) -> Result<bool, ApiError> {
        let db = Arc::clone(&self.db);
        self.rt
            .run_blocking(move || {
                let conn = db.reader()?;
                Ok(enabled_from(
                    repo::settings::get(&conn, ENABLED_KEY)?.as_deref(),
                ))
            })
            .await
    }

    /// Turns recording on or off. Existing entries are untouched — use [`Self::clear`] to purge.
    ///
    /// # Errors
    /// Returns [`ApiError::StorageCorrupt`] on a write failure.
    pub async fn set_enabled(&self, enabled: bool) -> Result<(), ApiError> {
        let db = Arc::clone(&self.db);
        self.rt
            .run_blocking(move || {
                let conn = db.writer();
                repo::settings::set(&conn, ENABLED_KEY, if enabled { "1" } else { "0" })?;
                Ok(())
            })
            .await
    }
}

/// The current time in Unix seconds, saturating rather than panicking on an out-of-range clock.
fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX)
        })
}

#[cfg(test)]
mod tests {
    use super::enabled_from;

    #[test]
    fn off_switch_defaults_on_and_only_zero_disables() {
        assert!(enabled_from(None)); // fresh install records by default
        assert!(enabled_from(Some("1")));
        assert!(!enabled_from(Some("0")));
    }
}
