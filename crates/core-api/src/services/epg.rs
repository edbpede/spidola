// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Bounded EPG ingest and query service (PRD §6.6).

use std::collections::HashMap;
use std::sync::Arc;

use core_db::{Db, EpgCommit, EpgStaging, repo};
use core_fetch::{FetchConfig, HttpClient, RequestSpec};
use core_model::{
    ChannelIdentity, EpgEntry, EpgEntryId, Secret, SecretRef, Source as DomainSource, SourceId,
    StreamLocator,
};
use core_parse::{EpgWindow, ParsedProgramme, ProgrammeSink, XmltvParseError, XmltvParser};
use rand::Rng as _;
use tokio::sync::mpsc;
use tracing::{instrument, warn};

use crate::error::{ApiError, InputField, InputIssue};
use crate::events::{CancelToken, TaskHandle};
use crate::logging::targets;
use crate::records::{EpgPage, EpgProgramme, NowNext};
use crate::runtime::CoreRuntime;
use crate::secrets::SecretStore;
use crate::settings::{AppSettings, count_from, keys};

const CHUNK_CHANNEL_DEPTH: usize = 8;
const STAGING_BATCH_SIZE: usize = 256;
type IdentityMap = HashMap<String, Vec<ChannelIdentity>>;

/// Stage of a running guide refresh.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum EpgRefreshStage {
    /// Resolving the configured feed and opening the connection.
    Connecting,
    /// Streaming and parsing XMLTV.
    Downloading,
    /// Atomically replacing the rolling schedule.
    Finalizing,
}

/// Progress from a running guide refresh.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct EpgRefreshProgress {
    pub stage: EpgRefreshStage,
    pub programmes_seen: u64,
}

/// Terminal result of a committed guide refresh.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct EpgRefreshOutcome {
    pub inserted: u64,
    pub emitted: u64,
    pub skipped: u64,
    pub unmapped: u64,
}

/// Listener for a long-running guide refresh. Calls may arrive on any core thread.
#[uniffi::export(callback_interface)]
pub trait EpgRefreshListener: Send + Sync {
    fn on_progress(&self, progress: EpgRefreshProgress);
    fn on_complete(&self, outcome: EpgRefreshOutcome);
    fn on_failed(&self, error: ApiError);
}

/// Manages source-scoped guide feeds and reads the rolling EPG store.
#[derive(uniffi::Object)]
pub struct EpgService {
    rt: Arc<CoreRuntime>,
    db: Arc<Db>,
    secrets: Arc<dyn SecretStore>,
}

impl EpgService {
    pub(crate) fn new(
        rt: Arc<CoreRuntime>,
        db: Arc<Db>,
        secrets: Arc<dyn SecretStore>,
    ) -> Arc<Self> {
        Arc::new(Self { rt, db, secrets })
    }
}

#[uniffi::export]
impl EpgService {
    /// Stores an XMLTV URL for an M3U source. The URL lives only in platform secure storage.
    ///
    /// # Errors
    /// Returns an input, not-found, secure-store, or storage error.
    #[instrument(skip(self, url), fields(source_id), err)]
    pub async fn set_xmltv_feed(&self, source_id: i64, url: String) -> Result<(), ApiError> {
        StreamLocator::parse(&url)?;
        let db = Arc::clone(&self.db);
        let secrets = Arc::clone(&self.secrets);
        self.rt
            .run_blocking(move || {
                let id = SourceId::new(source_id);
                let source = {
                    let conn = db.reader()?;
                    repo::sources::get(&conn, id)?.ok_or(ApiError::NotFound)?
                };
                if matches!(source, DomainSource::Xtream { .. }) {
                    return Err(ApiError::InvalidInput {
                        field: InputField::Source,
                        issue: InputIssue::Unsupported,
                    });
                }

                let previous = {
                    let conn = db.reader()?;
                    repo::epg::get_feed(&conn, id)?
                };
                let next = mint_feed_ref();
                secrets.set(next.as_str().to_owned(), url)?;
                let write = {
                    let conn = db.writer();
                    repo::epg::set_feed(&conn, id, &next)
                };
                if let Err(error) = write {
                    let _ = secrets.delete(next.as_str().to_owned());
                    return Err(ApiError::from(error));
                }
                if let Some(previous) = previous
                    && let Err(error) = secrets.delete(previous.as_str().to_owned())
                {
                    let conn = db.writer();
                    repo::epg::set_feed(&conn, id, &previous)?;
                    let _ = secrets.delete(next.as_str().to_owned());
                    return Err(error);
                }
                Ok(())
            })
            .await
    }

    /// Whether this source can refresh a guide. Xtream accounts provide their own endpoint.
    ///
    /// # Errors
    /// Returns `NotFound` or a storage error.
    pub async fn has_feed(&self, source_id: i64) -> Result<bool, ApiError> {
        let db = Arc::clone(&self.db);
        self.rt
            .run_blocking(move || {
                let conn = db.reader()?;
                let id = SourceId::new(source_id);
                let source = repo::sources::get(&conn, id)?.ok_or(ApiError::NotFound)?;
                Ok(matches!(source, DomainSource::Xtream { .. })
                    || repo::epg::get_feed(&conn, id)?.is_some())
            })
            .await
    }

    /// Removes a configured XMLTV feed and its secure-store value.
    ///
    /// # Errors
    /// Returns a secure-store or storage error.
    pub async fn clear_xmltv_feed(&self, source_id: i64) -> Result<(), ApiError> {
        let db = Arc::clone(&self.db);
        let secrets = Arc::clone(&self.secrets);
        self.rt
            .run_blocking(move || {
                let id = SourceId::new(source_id);
                let feed = {
                    let conn = db.reader()?;
                    repo::epg::get_feed(&conn, id)?
                };
                let Some(feed) = feed else {
                    return Ok(());
                };
                let stored = secrets.get(feed.as_str().to_owned())?;
                secrets.delete(feed.as_str().to_owned())?;
                let result = {
                    let conn = db.writer();
                    repo::epg::remove_feed(&conn, id)
                };
                if let Err(error) = result {
                    if let Some(stored) = stored {
                        let _ = secrets.set(feed.as_str().to_owned(), stored);
                    }
                    return Err(ApiError::from(error));
                }
                Ok(())
            })
            .await
    }

    /// Refreshes XMLTV in the background with cancellation at parser batch boundaries.
    #[must_use]
    pub fn refresh(
        &self,
        source_id: i64,
        now_unix: i64,
        listener: Box<dyn EpgRefreshListener>,
    ) -> Arc<TaskHandle> {
        let token = CancelToken::default();
        let task_token = token.clone();
        let db = Arc::clone(&self.db);
        let secrets = Arc::clone(&self.secrets);
        let listener: Arc<dyn EpgRefreshListener> = Arc::from(listener);
        self.rt.spawn(async move {
            run_refresh(
                db,
                secrets,
                SourceId::new(source_id),
                now_unix,
                task_token,
                listener,
            )
            .await;
        });
        Arc::new(TaskHandle::new(token))
    }

    /// Returns current and next programme for one channel.
    ///
    /// # Errors
    /// Returns a storage error if the rolling guide cannot be read.
    #[instrument(skip(self), fields(source_id, channel_identity), err)]
    pub async fn now_next(
        &self,
        source_id: i64,
        channel_identity: i64,
        now_unix: i64,
    ) -> Result<NowNext, ApiError> {
        let db = Arc::clone(&self.db);
        self.rt
            .run_blocking(move || {
                let conn = db.reader()?;
                let (current, next) = repo::epg::now_next(
                    &conn,
                    SourceId::new(source_id),
                    ChannelIdentity::from_storage(channel_identity),
                    now_unix,
                )?;
                Ok(NowNext {
                    current: current.map(EpgProgramme::from),
                    next: next.map(EpgProgramme::from),
                })
            })
            .await
    }

    /// Returns a bounded page intersecting the requested time window.
    ///
    /// # Errors
    /// Returns a storage error if the rolling guide cannot be read.
    #[allow(clippy::too_many_arguments)]
    pub async fn window(
        &self,
        source_id: i64,
        channel_identity: i64,
        earliest_unix: i64,
        latest_unix: i64,
        offset: u32,
        limit: u32,
    ) -> Result<EpgPage, ApiError> {
        let db = Arc::clone(&self.db);
        self.rt
            .run_blocking(move || {
                let conn = db.reader()?;
                let programmes = repo::epg::list_window(
                    &conn,
                    SourceId::new(source_id),
                    ChannelIdentity::from_storage(channel_identity),
                    earliest_unix,
                    latest_unix,
                    offset,
                    limit,
                )?;
                Ok(EpgPage {
                    programmes: programmes.into_iter().map(EpgProgramme::from).collect(),
                    offset,
                })
            })
            .await
    }
}

enum Feed {
    Xmltv {
        url: String,
        accept_invalid_tls: bool,
    },
    Xtream {
        server: StreamLocator,
        username: String,
        password: String,
    },
}

struct PreparedRefresh {
    feed: Feed,
    identities: IdentityMap,
    window: EpgWindow,
}

async fn run_refresh(
    db: Arc<Db>,
    secrets: Arc<dyn SecretStore>,
    source: SourceId,
    now_unix: i64,
    token: CancelToken,
    listener: Arc<dyn EpgRefreshListener>,
) {
    listener.on_progress(EpgRefreshProgress {
        stage: EpgRefreshStage::Connecting,
        programmes_seen: 0,
    });
    let prepared = {
        let db = Arc::clone(&db);
        tokio::task::spawn_blocking(move || prepare_refresh(&db, &secrets, source)).await
    };
    let prepared = match prepared {
        Ok(Ok(prepared)) => prepared,
        Ok(Err(error)) => {
            listener.on_failed(error);
            return;
        }
        Err(join) => {
            warn!(target: targets::IMPORT, cause = %join, "EPG preparation task failed to join");
            listener.on_failed(ApiError::Internal);
            return;
        }
    };
    if token.is_cancelled() {
        listener.on_failed(ApiError::Cancelled);
        return;
    }

    let response = match fetch_feed(prepared.feed).await {
        Ok(response) => response,
        Err(error) => {
            listener.on_failed(error);
            return;
        }
    };
    listener.on_progress(EpgRefreshProgress {
        stage: EpgRefreshStage::Downloading,
        programmes_seen: 0,
    });

    match stream_and_stage(
        Arc::clone(&db),
        source,
        now_unix,
        prepared.window,
        prepared.identities,
        response,
        token,
        Arc::clone(&listener),
    )
    .await
    {
        Ok(StageResult::Completed(outcome)) => listener.on_complete(outcome),
        Ok(StageResult::Cancelled) => listener.on_failed(ApiError::Cancelled),
        Err(error) => listener.on_failed(error),
    }
}

fn prepare_refresh(
    db: &Db,
    secrets: &Arc<dyn SecretStore>,
    source: SourceId,
) -> Result<PreparedRefresh, ApiError> {
    let conn = db.reader()?;
    let source_record = repo::sources::get(&conn, source)?.ok_or(ApiError::NotFound)?;
    let feed = match source_record {
        DomainSource::M3uUrl {
            accept_invalid_tls, ..
        } => Feed::Xmltv {
            url: load_xmltv_url(repo::epg::get_feed(&conn, source)?, secrets)?,
            accept_invalid_tls,
        },
        DomainSource::M3uFile { .. } => Feed::Xmltv {
            url: load_xmltv_url(repo::epg::get_feed(&conn, source)?, secrets)?,
            accept_invalid_tls: false,
        },
        DomainSource::Xtream {
            server,
            username,
            secret,
            ..
        } => Feed::Xtream {
            server,
            username,
            password: secrets
                .get(secret.as_str().to_owned())?
                .ok_or(ApiError::Unauthorized)?,
        },
    };
    let defaults = AppSettings::default();
    let ahead = count_from(
        repo::settings::get(&conn, keys::EPG_WINDOW_AHEAD_HOURS)?.as_deref(),
        defaults.epg_window_ahead_hours,
    );
    let behind = count_from(
        repo::settings::get(&conn, keys::EPG_WINDOW_BEHIND_HOURS)?.as_deref(),
        defaults.epg_window_behind_hours,
    );
    Ok(PreparedRefresh {
        feed,
        identities: repo::channels::epg_identity_map(&conn, source)?,
        window: EpgWindow::from_hours(behind, ahead),
    })
}

fn load_xmltv_url(
    feed: Option<SecretRef>,
    secrets: &Arc<dyn SecretStore>,
) -> Result<String, ApiError> {
    let feed = feed.ok_or(ApiError::InvalidInput {
        field: InputField::Source,
        issue: InputIssue::Unavailable,
    })?;
    secrets
        .get(feed.as_str().to_owned())?
        .ok_or(ApiError::InvalidInput {
            field: InputField::Source,
            issue: InputIssue::Unavailable,
        })
}

async fn fetch_feed(feed: Feed) -> Result<core_fetch::Response, ApiError> {
    match feed {
        Feed::Xmltv {
            url,
            accept_invalid_tls,
        } => {
            let client = HttpClient::new(&FetchConfig {
                accept_invalid_tls,
                ..FetchConfig::default()
            })?;
            client
                .get(&RequestSpec::new(&url))
                .await
                .map_err(ApiError::from)
        }
        Feed::Xtream {
            server,
            username,
            password,
        } => {
            let client = HttpClient::new(&FetchConfig::default())?;
            let endpoint =
                core_xtream::Endpoint::new(&server, &username).map_err(crate::xtream::map_error)?;
            let password = Secret::new(password);
            let url = endpoint
                .xmltv(&password)
                .map_err(crate::xtream::map_error)?;
            let response = client
                .get(&RequestSpec::new(url.expose()))
                .await
                .map_err(ApiError::from);
            drop(url);
            response
        }
    }
}

enum StageResult {
    Completed(EpgRefreshOutcome),
    Cancelled,
}

#[allow(clippy::too_many_arguments)]
async fn stream_and_stage(
    db: Arc<Db>,
    source: SourceId,
    now_unix: i64,
    window: EpgWindow,
    identities: IdentityMap,
    mut response: core_fetch::Response,
    token: CancelToken,
    listener: Arc<dyn EpgRefreshListener>,
) -> Result<StageResult, ApiError> {
    let (sender, receiver) = mpsc::channel::<Vec<u8>>(CHUNK_CHANNEL_DEPTH);
    let worker = {
        let db = Arc::clone(&db);
        let worker_token = token.clone();
        tokio::task::spawn_blocking(move || {
            parse_and_stage(
                &db,
                source,
                now_unix,
                window,
                &identities,
                receiver,
                &worker_token,
                &listener,
            )
        })
    };
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| ApiError::from(core_fetch::classify(error)))?
    {
        if token.is_cancelled() || sender.send(chunk.to_vec()).await.is_err() {
            break;
        }
    }
    drop(sender);
    worker.await.map_err(|join| {
        warn!(target: targets::IMPORT, cause = %join, "EPG staging task failed to join");
        ApiError::Internal
    })?
}

#[allow(clippy::too_many_arguments)]
fn parse_and_stage(
    db: &Db,
    source: SourceId,
    now_unix: i64,
    window: EpgWindow,
    identities: &IdentityMap,
    mut receiver: mpsc::Receiver<Vec<u8>>,
    token: &CancelToken,
    listener: &Arc<dyn EpgRefreshListener>,
) -> Result<StageResult, ApiError> {
    let mut staging = db.begin_epg_staging(source)?;
    let mut parser = XmltvParser::new(now_unix, window);
    let mut sink = StagingSink {
        staging: &mut staging,
        source,
        identities,
        token,
        listener,
        programmes_seen: 0,
        mapped: 0,
        unmapped: 0,
        cancelled: false,
    };
    while let Some(chunk) = receiver.blocking_recv() {
        if token.is_cancelled() {
            sink.cancelled = true;
            break;
        }
        parser.push(&chunk, &mut sink).map_err(map_parse_error)?;
        if sink.cancelled {
            break;
        }
    }
    if sink.cancelled || token.is_cancelled() {
        return Ok(StageResult::Cancelled);
    }
    let diagnostics = parser.finish(&mut sink).map_err(map_parse_error)?;
    let mapped = sink.mapped;
    let unmapped = sink.unmapped;
    if diagnostics.total_programmes() == 0 || mapped == 0 {
        return Err(ApiError::ParseFailed {
            emitted: diagnostics.emitted(),
            skipped: diagnostics.skipped().saturating_add(unmapped),
        });
    }
    listener.on_progress(EpgRefreshProgress {
        stage: EpgRefreshStage::Finalizing,
        programmes_seen: sink.programmes_seen,
    });
    match staging.commit(db)? {
        EpgCommit::Committed { inserted } => Ok(StageResult::Completed(EpgRefreshOutcome {
            inserted,
            emitted: diagnostics.emitted(),
            skipped: diagnostics.skipped(),
            unmapped,
        })),
        EpgCommit::SourceRemoved => Ok(StageResult::Cancelled),
    }
}

struct StagingSink<'a> {
    staging: &'a mut EpgStaging,
    source: SourceId,
    identities: &'a IdentityMap,
    token: &'a CancelToken,
    listener: &'a Arc<dyn EpgRefreshListener>,
    programmes_seen: u64,
    mapped: u64,
    unmapped: u64,
    cancelled: bool,
}

impl ProgrammeSink for StagingSink<'_> {
    type Error = ApiError;

    fn accept(&mut self, batch: Vec<ParsedProgramme>) -> Result<(), Self::Error> {
        if self.cancelled || self.token.is_cancelled() {
            self.cancelled = true;
            return Ok(());
        }
        let batch_len = u64::try_from(batch.len()).unwrap_or(u64::MAX);
        let mut entries = Vec::with_capacity(STAGING_BATCH_SIZE);
        for programme in batch {
            let Some(identities) = self.identities.get(&programme.channel) else {
                self.unmapped = self.unmapped.saturating_add(1);
                continue;
            };
            for identity in identities {
                entries.push(EpgEntry {
                    id: EpgEntryId::new(0),
                    source_id: self.source,
                    channel: *identity,
                    title: programme.title.clone(),
                    description: programme.description.clone(),
                    start_unix: programme.start_unix,
                    end_unix: programme.end_unix,
                });
                if entries.len() == STAGING_BATCH_SIZE {
                    self.staging.stage(&entries)?;
                    self.mapped = self
                        .mapped
                        .saturating_add(u64::try_from(entries.len()).unwrap_or(u64::MAX));
                    entries.clear();
                }
            }
        }
        if !entries.is_empty() {
            self.staging.stage(&entries)?;
            self.mapped = self
                .mapped
                .saturating_add(u64::try_from(entries.len()).unwrap_or(u64::MAX));
        }
        self.programmes_seen = self.programmes_seen.saturating_add(batch_len);
        self.listener.on_progress(EpgRefreshProgress {
            stage: EpgRefreshStage::Downloading,
            programmes_seen: self.programmes_seen,
        });
        Ok(())
    }
}

fn map_parse_error(error: XmltvParseError<ApiError>) -> ApiError {
    match error {
        XmltvParseError::Sink(error) => error,
    }
}

fn mint_feed_ref() -> SecretRef {
    SecretRef::new(format!(
        "spidola/epg/{:016x}/{:016x}",
        rand::rng().random::<u64>(),
        rand::rng().random::<u64>()
    ))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn feed_references_are_opaque_and_unique() {
        let first = mint_feed_ref();
        let second = mint_feed_ref();
        assert_ne!(first, second);
        assert!(first.as_str().starts_with("spidola/epg/"));
    }
}
