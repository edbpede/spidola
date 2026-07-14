// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! `CatalogService`: browse queries, paged by contract (TECH_SPEC §4.6). Every list-returning
//! method takes an offset/limit, so no unbounded collection ever crosses the boundary.

use std::sync::Arc;

use core_db::{Db, repo};
use core_model::channel::MediaKind as DomainMediaKind;
use core_model::ids::{ChannelId, SourceId};

use crate::error::ApiError;
use crate::records::{
    BrowseGroup, BrowseGroupPage, Channel, ChannelPage, MediaKind, identity_from_storage,
};
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

    /// Lists the media kinds present in a source's catalog, in display order — the "type" level
    /// of the browse drill-down (source → type → category → channel). For an M3U source this is
    /// just `[Live]`, so a shell may skip the type screen when only one kind exists.
    ///
    /// # Errors
    /// Returns [`ApiError::StorageCorrupt`] on a query failure.
    pub async fn kinds(&self, source_id: i64) -> Result<Vec<MediaKind>, ApiError> {
        let db = Arc::clone(&self.db);
        self.rt
            .run_blocking(move || {
                let conn = db.reader()?;
                let kinds = repo::channels::kinds_for_source(&conn, SourceId::new(source_id))?;
                Ok(kinds.into_iter().map(MediaKind::from).collect())
            })
            .await
    }

    /// Returns a page of a source's distinct groups (categories) for a media kind, ungrouped
    /// last (paged by contract). Hidden channels are excluded from the counts.
    ///
    /// # Errors
    /// Returns [`ApiError::StorageCorrupt`] on a query failure.
    pub async fn groups(
        &self,
        source_id: i64,
        kind: MediaKind,
        offset: u32,
        limit: u32,
    ) -> Result<BrowseGroupPage, ApiError> {
        let db = Arc::clone(&self.db);
        self.rt
            .run_blocking(move || {
                let source = SourceId::new(source_id);
                let domain_kind = DomainMediaKind::from(kind);
                let conn = db.reader()?;
                let total = repo::channels::count_groups(&conn, source, domain_kind)?;
                let groups =
                    repo::channels::browse_groups(&conn, source, domain_kind, offset, limit)?
                        .into_iter()
                        .map(|group| BrowseGroup {
                            title: group.title,
                            channel_count: group.channel_count,
                        })
                        .collect();
                Ok(BrowseGroupPage {
                    groups,
                    offset,
                    total,
                })
            })
            .await
    }

    /// Returns a page of the visible channels in one group of a source and media kind, in
    /// playlist order (paged by contract). `group` is the group title; `None` selects the
    /// "ungrouped" bucket. Hidden channels are excluded.
    ///
    /// # Errors
    /// Returns [`ApiError::StorageCorrupt`] on a query failure.
    // UniFFI lifts `group` into an owned `Option<String>`; the query only borrows it.
    #[allow(clippy::needless_pass_by_value)]
    pub async fn channels_in_group(
        &self,
        source_id: i64,
        kind: MediaKind,
        group: Option<String>,
        offset: u32,
        limit: u32,
    ) -> Result<ChannelPage, ApiError> {
        let db = Arc::clone(&self.db);
        self.rt
            .run_blocking(move || {
                let source = SourceId::new(source_id);
                let domain_kind = DomainMediaKind::from(kind);
                let group = group.as_deref();
                let conn = db.reader()?;
                let total = repo::channels::count_in_group(&conn, source, domain_kind, group)?;
                let channels = repo::channels::list_in_group(
                    &conn,
                    source,
                    domain_kind,
                    group,
                    offset,
                    limit,
                )?;
                Ok(ChannelPage {
                    channels: channels.into_iter().map(Channel::from).collect(),
                    offset,
                    total,
                })
            })
            .await
    }

    /// Hides or unhides a channel by its stable identity (the browse context menu). Hidden
    /// channels are excluded from [`Self::groups`] and [`Self::channels_in_group`], and the flag
    /// survives a refresh (§4.4).
    ///
    /// # Errors
    /// Returns [`ApiError::StorageCorrupt`] on a write failure.
    pub async fn set_hidden(
        &self,
        source_id: i64,
        identity: i64,
        hidden: bool,
    ) -> Result<(), ApiError> {
        let db = Arc::clone(&self.db);
        self.rt
            .run_blocking(move || {
                let source = SourceId::new(source_id);
                let ident = identity_from_storage(identity);
                let conn = db.writer();
                if hidden {
                    repo::channels::hide(&conn, source, ident)?;
                } else {
                    repo::channels::unhide(&conn, source, ident)?;
                }
                Ok(())
            })
            .await
    }

    /// Whether a channel is hidden.
    ///
    /// # Errors
    /// Returns [`ApiError::StorageCorrupt`] on a query failure.
    pub async fn is_hidden(&self, source_id: i64, identity: i64) -> Result<bool, ApiError> {
        let db = Arc::clone(&self.db);
        self.rt
            .run_blocking(move || {
                let conn = db.reader()?;
                Ok(repo::channels::is_hidden(
                    &conn,
                    SourceId::new(source_id),
                    identity_from_storage(identity),
                )?)
            })
            .await
    }
}
