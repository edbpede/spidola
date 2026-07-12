// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! `SourceService`: add, list, refresh (with progress), rename, disable, delete
//! (TECH_SPEC §4.6). Xtream add is stubbed until Phase 6; this phase covers M3U-by-URL, the
//! flow the boundary exit criteria exercise.

use std::sync::Arc;

use core_db::{Db, repo};
use core_model::ids::SourceId;
use core_model::locator::StreamLocator;
use core_model::source::{Source as DomainSource, SourceCommon as DomainCommon};

use crate::error::ApiError;
use crate::events::{CancelToken, ImportListener, TaskHandle};
use crate::import::run_import;
use crate::records::Source;
use crate::runtime::CoreRuntime;

/// Manages configured sources and their catalog refresh.
#[derive(uniffi::Object)]
pub struct SourceService {
    rt: Arc<CoreRuntime>,
    db: Arc<Db>,
}

impl SourceService {
    /// Builds the service over shared runtime and database handles.
    pub(crate) fn new(rt: Arc<CoreRuntime>, db: Arc<Db>) -> Arc<Self> {
        Arc::new(Self { rt, db })
    }
}

#[uniffi::export]
impl SourceService {
    /// Lists all configured sources, newest first.
    ///
    /// # Errors
    /// Returns [`ApiError::StorageCorrupt`] if the source list cannot be read.
    pub async fn list(&self) -> Result<Vec<Source>, ApiError> {
        let db = Arc::clone(&self.db);
        self.rt
            .run_blocking(move || {
                let conn = db.reader()?;
                let sources = repo::sources::list(&conn)?;
                Ok(sources.into_iter().map(Source::from).collect())
            })
            .await
    }

    /// Adds an M3U-by-URL source (no import yet — call [`Self::refresh`] to fetch its catalog).
    ///
    /// # Errors
    /// Returns [`ApiError::InvalidInput`] if `url` is not a valid absolute URL, or
    /// [`ApiError::StorageCorrupt`] if the source cannot be persisted.
    pub async fn add_m3u_url(
        &self,
        name: String,
        url: String,
        user_agent: Option<String>,
        accept_invalid_tls: bool,
    ) -> Result<Source, ApiError> {
        let db = Arc::clone(&self.db);
        self.rt
            .run_blocking(move || {
                let locator = StreamLocator::parse(&url)?; // parse, don't validate
                let source = DomainSource::M3uUrl {
                    id: SourceId::new(0), // the DB mints the rowid
                    common: DomainCommon {
                        name,
                        enabled: true,
                        auto_refresh_secs: None,
                    },
                    url: locator,
                    user_agent,
                    accept_invalid_tls,
                };
                let id = {
                    let conn = db.writer();
                    repo::sources::insert(&conn, &source)?
                };
                let created = {
                    let conn = db.writer();
                    repo::sources::get(&conn, id)?.ok_or(ApiError::Internal)?
                };
                Ok(Source::from(created))
            })
            .await
    }

    /// Renames a source.
    ///
    /// # Errors
    /// Returns [`ApiError::StorageCorrupt`] on a write failure.
    pub async fn rename(&self, id: i64, name: String) -> Result<(), ApiError> {
        let db = Arc::clone(&self.db);
        self.rt
            .run_blocking(move || {
                let conn = db.writer();
                repo::sources::rename(&conn, SourceId::new(id), &name)?;
                Ok(())
            })
            .await
    }

    /// Enables or disables a source without deleting it.
    ///
    /// # Errors
    /// Returns [`ApiError::StorageCorrupt`] on a write failure.
    pub async fn set_enabled(&self, id: i64, enabled: bool) -> Result<(), ApiError> {
        let db = Arc::clone(&self.db);
        self.rt
            .run_blocking(move || {
                let conn = db.writer();
                repo::sources::set_enabled(&conn, SourceId::new(id), enabled)?;
                Ok(())
            })
            .await
    }

    /// Sets (or clears, with `None`) the automatic refresh interval in seconds.
    ///
    /// # Errors
    /// Returns [`ApiError::StorageCorrupt`] on a write failure.
    pub async fn set_auto_refresh(&self, id: i64, secs: Option<u32>) -> Result<(), ApiError> {
        let db = Arc::clone(&self.db);
        self.rt
            .run_blocking(move || {
                let conn = db.writer();
                repo::sources::set_auto_refresh(&conn, SourceId::new(id), secs)?;
                Ok(())
            })
            .await
    }

    /// Deletes a source and (by cascade) its catalog, favorites, hidden flags, and history.
    ///
    /// # Errors
    /// Returns [`ApiError::StorageCorrupt`] on a write failure.
    pub async fn delete(&self, id: i64) -> Result<(), ApiError> {
        let db = Arc::clone(&self.db);
        self.rt
            .run_blocking(move || {
                let conn = db.writer();
                repo::sources::delete(&conn, SourceId::new(id))?;
                Ok(())
            })
            .await
    }

    /// Refreshes a source's catalog from its URL. Returns immediately with a [`TaskHandle`];
    /// progress, completion, and failure arrive on `listener`. Cancellation via the handle is
    /// honest — checked at batch boundaries — and leaves the prior catalog intact on abort.
    #[must_use]
    pub fn refresh(&self, id: i64, listener: Box<dyn ImportListener>) -> Arc<TaskHandle> {
        let token = CancelToken::default();
        let db = Arc::clone(&self.db);
        let listener: Arc<dyn ImportListener> = Arc::from(listener);
        let task_token = token.clone();
        self.rt.spawn(async move {
            run_refresh(db, SourceId::new(id), task_token, listener).await;
        });
        Arc::new(TaskHandle::new(token))
    }
}

/// Reads a source's fetch parameters (on the runtime), then streams its catalog import.
async fn run_refresh(
    db: Arc<Db>,
    source_id: SourceId,
    token: CancelToken,
    listener: Arc<dyn ImportListener>,
) {
    let read = {
        let db = Arc::clone(&db);
        tokio::task::spawn_blocking(move || read_source(&db, source_id)).await
    };
    match read {
        Ok(Ok(Some(DomainSource::M3uUrl {
            url,
            user_agent,
            accept_invalid_tls,
            ..
        }))) => {
            run_import(
                db,
                source_id,
                url.to_string(),
                user_agent,
                accept_invalid_tls,
                token,
                listener,
            )
            .await;
        }
        Ok(Ok(Some(_))) => listener.on_failed(ApiError::InvalidInput {
            reason: "this source can't be refreshed from a URL yet".to_owned(),
        }),
        Ok(Ok(None)) => listener.on_failed(ApiError::NotFound),
        Ok(Err(error)) => listener.on_failed(error),
        Err(_) => listener.on_failed(ApiError::Internal),
    }
}

/// Blocking read of one source's persisted definition.
fn read_source(db: &Db, id: SourceId) -> Result<Option<DomainSource>, ApiError> {
    let conn = db.reader()?;
    Ok(repo::sources::get(&conn, id)?)
}
