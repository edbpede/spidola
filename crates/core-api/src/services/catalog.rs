// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! `CatalogService`: browse queries, paged by contract (TECH_SPEC §4.6). Every list-returning
//! method takes an offset/limit, so no unbounded collection ever crosses the boundary.

use std::sync::Arc;

use core_db::{Db, repo};
use core_model::ids::{ChannelId, SourceId};

use crate::error::ApiError;
use crate::records::{Channel, ChannelPage};
use crate::runtime::CoreRuntime;

/// Reads a source's channel catalog for browse.
#[derive(uniffi::Object)]
pub struct CatalogService {
    rt: Arc<CoreRuntime>,
    db: Arc<Db>,
}

impl CatalogService {
    /// Builds the service over shared runtime and database handles.
    pub(crate) fn new(rt: Arc<CoreRuntime>, db: Arc<Db>) -> Arc<Self> {
        Arc::new(Self { rt, db })
    }
}

#[uniffi::export]
impl CatalogService {
    /// Counts the channels in a source's current catalog.
    ///
    /// # Errors
    /// Returns [`ApiError::StorageCorrupt`] on a query failure.
    pub async fn channel_count(&self, source_id: i64) -> Result<u64, ApiError> {
        let db = Arc::clone(&self.db);
        self.rt
            .run_blocking(move || {
                let conn = db.reader()?;
                Ok(repo::channels::count_for_source(
                    &conn,
                    SourceId::new(source_id),
                )?)
            })
            .await
    }

    /// Returns a page of a source's channels in playlist order (paged by contract).
    ///
    /// # Errors
    /// Returns [`ApiError::StorageCorrupt`] on a query failure.
    pub async fn channels(
        &self,
        source_id: i64,
        offset: u32,
        limit: u32,
    ) -> Result<ChannelPage, ApiError> {
        let db = Arc::clone(&self.db);
        self.rt
            .run_blocking(move || {
                let source = SourceId::new(source_id);
                let conn = db.reader()?;
                let total = repo::channels::count_for_source(&conn, source)?;
                let channels = repo::channels::list_for_source(&conn, source, offset, limit)?;
                Ok(ChannelPage {
                    channels: channels.into_iter().map(Channel::from).collect(),
                    offset,
                    total,
                })
            })
            .await
    }

    /// Fetches one channel by rowid.
    ///
    /// # Errors
    /// Returns [`ApiError::StorageCorrupt`] on a query failure.
    pub async fn channel(&self, id: i64) -> Result<Option<Channel>, ApiError> {
        let db = Arc::clone(&self.db);
        self.rt
            .run_blocking(move || {
                let conn = db.reader()?;
                Ok(repo::channels::get(&conn, ChannelId::new(id))?.map(Channel::from))
            })
            .await
    }
}
