// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! `FavoritesService` (TECH_SPEC §4.6, PRD §6.5). Favorites key on the stable channel
//! identity so they survive a refresh. The `core-db` layer has no clock; the timestamp is
//! injected here, at the service boundary.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use core_db::{Db, repo};
use core_model::ids::SourceId;

use crate::error::ApiError;
use crate::records::{Channel, ChannelPage, Favorite, identity_from_storage};
use crate::runtime::CoreRuntime;

/// Manages favorite channels.
#[derive(uniffi::Object)]
pub struct FavoritesService {
    rt: Arc<CoreRuntime>,
    db: Arc<Db>,
}

impl FavoritesService {
    /// Builds the service over shared runtime and database handles.
    pub(crate) fn new(rt: Arc<CoreRuntime>, db: Arc<Db>) -> Arc<Self> {
        Arc::new(Self { rt, db })
    }
}

#[uniffi::export]
impl FavoritesService {
    /// Marks a channel favorite (idempotent). `identity` is the channel's stored identity.
    ///
    /// # Errors
    /// Returns [`ApiError::StorageCorrupt`] on a write failure.
    pub async fn add(&self, source_id: i64, identity: i64) -> Result<(), ApiError> {
        let db = Arc::clone(&self.db);
        let created_at = now_unix();
        self.rt
            .run_blocking(move || {
                let conn = db.writer();
                repo::favorites::add(
                    &conn,
                    SourceId::new(source_id),
                    identity_from_storage(identity),
                    created_at,
                )?;
                Ok(())
            })
            .await
    }

    /// Removes a favorite (idempotent).
    ///
    /// # Errors
    /// Returns [`ApiError::StorageCorrupt`] on a write failure.
    pub async fn remove(&self, source_id: i64, identity: i64) -> Result<(), ApiError> {
        let db = Arc::clone(&self.db);
        self.rt
            .run_blocking(move || {
                let conn = db.writer();
                repo::favorites::remove(
                    &conn,
                    SourceId::new(source_id),
                    identity_from_storage(identity),
                )?;
                Ok(())
            })
            .await
    }

    /// Whether a channel is favorited.
    ///
    /// # Errors
    /// Returns [`ApiError::StorageCorrupt`] on a query failure.
    pub async fn is_favorite(&self, source_id: i64, identity: i64) -> Result<bool, ApiError> {
        let db = Arc::clone(&self.db);
        self.rt
            .run_blocking(move || {
                let conn = db.reader()?;
                Ok(repo::favorites::is_favorite(
                    &conn,
                    SourceId::new(source_id),
                    identity_from_storage(identity),
                )?)
            })
            .await
    }

    /// Lists a source's favorites, most recently added first.
    ///
    /// # Errors
    /// Returns [`ApiError::StorageCorrupt`] on a query failure.
    pub async fn list(&self, source_id: i64) -> Result<Vec<Favorite>, ApiError> {
        let db = Arc::clone(&self.db);
        self.rt
            .run_blocking(move || {
                let conn = db.reader()?;
                let favorites = repo::favorites::list_for_source(&conn, SourceId::new(source_id))?;
                Ok(favorites.into_iter().map(Favorite::from).collect())
            })
            .await
    }

    /// Returns a page of favorited channels across all enabled sources, most recently favorited
    /// first — the home "Favorites" row (PRD §8.3). Each favorite is resolved to the channel in
    /// the current catalog by stable identity; favorites whose channel is absent or whose source
    /// is disabled are omitted. Paged by contract (§4.6).
    ///
    /// # Errors
    /// Returns [`ApiError::StorageCorrupt`] on a query failure.
    pub async fn favorite_channels(
        &self,
        offset: u32,
        limit: u32,
    ) -> Result<ChannelPage, ApiError> {
        let db = Arc::clone(&self.db);
        self.rt
            .run_blocking(move || {
                let conn = db.reader()?;
                let total = repo::favorites::count_channels(&conn)?;
                let channels = repo::favorites::list_channels(&conn, offset, limit)?;
                Ok(ChannelPage {
                    channels: channels.into_iter().map(Channel::from).collect(),
                    offset,
                    total,
                })
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
