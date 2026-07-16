// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The streaming M3U import pipeline (fetch → parse → staging-and-swap), owned by
//! `SourceService` (TECH_SPEC §4.6).
//!
//! Bytes flow network → parser → DB batch with no full buffering: the async side pulls the
//! HTTP body one chunk at a time on the core runtime and hands each chunk over a small bounded
//! channel to a blocking staging worker that drives the parser and the staging-and-swap
//! transaction. Peak memory stays bounded to one batch regardless of playlist size. Progress
//! is reported per staged batch, and cancellation is checked at every batch boundary; a
//! cancelled or failed import drops the transaction un-committed, so the prior catalog survives
//! intact (TECH_SPEC §4.4).

use std::sync::Arc;

use core_db::{Db, NewChannel, RefreshCommit, Staging};
use core_fetch::{FetchConfig, HttpClient, RequestSpec};
use core_model::channel::{ChannelOverrides, MediaKind, channel_identity};
use core_model::ids::SourceId;
use core_model::locator::StreamLocator;
use core_parse::{ChannelSink, M3uParser, ParseError, ParsedChannel};
use tokio::sync::mpsc;
use tracing::{debug, info, info_span, warn};

use crate::error::ApiError;
use crate::events::{CancelToken, ImportListener, ImportOutcome, ImportProgress, ImportStage};
use crate::logging::targets;
use crate::storage_crypto::CatalogCipher;

/// Bounded channel depth bridging the async fetch and the blocking staging worker.
const CHUNK_CHANNEL_DEPTH: usize = 8;

/// Chunk size for feeding an in-memory playlist (a picked/pasted file) to the parser, so the
/// content path keeps the same bounded-memory parser behaviour as the streaming URL path.
const CONTENT_CHUNK_SIZE: usize = 64 * 1024;

/// The terminal result of a staging run.
enum StagingResult {
    /// Committed; the new catalog is live.
    Completed(ImportOutcome),
    /// Cancelled at a batch boundary; the transaction was dropped un-committed.
    Cancelled,
}

/// Runs a full import for `url` into `source_id`, reporting to `listener` and honouring
/// `token`. Intended to be driven as a task on the core runtime; it never panics across the
/// boundary — every failure path ends in exactly one terminal `listener` call.
pub(crate) async fn run_import(
    db: Arc<Db>,
    cipher: Arc<CatalogCipher>,
    source_id: SourceId,
    url: String,
    user_agent: Option<String>,
    accept_invalid_tls: bool,
    token: CancelToken,
    listener: Arc<dyn ImportListener>,
) {
    listener.on_progress(ImportProgress {
        stage: ImportStage::Connecting,
        channels_seen: 0,
    });
    match import_inner(
        &db,
        &cipher,
        source_id,
        &url,
        user_agent,
        accept_invalid_tls,
        &token,
        &listener,
    )
    .await
    {
        Ok(StagingResult::Completed(outcome)) => listener.on_complete(outcome),
        Ok(StagingResult::Cancelled) => listener.on_failed(ApiError::Cancelled),
        Err(error) => listener.on_failed(error),
    }
}

/// Runs an import from an in-memory M3U `content` string (a pasted playlist or a picked/SAF
/// local file the shell has already read) into `source_id`, reporting to `listener` and
/// honouring `token`. Same staging-and-swap and bounded-memory guarantees as the URL path: the
/// content is fed to the parser one chunk at a time, so peak parser memory stays bounded to one
/// batch regardless of file size. Intended to be driven as a task on the core runtime; it never
/// panics across the boundary — every failure path ends in exactly one terminal `listener` call.
pub(crate) async fn run_import_content(
    db: Arc<Db>,
    cipher: Arc<CatalogCipher>,
    source_id: SourceId,
    content: Vec<u8>,
    token: CancelToken,
    listener: Arc<dyn ImportListener>,
) {
    listener.on_progress(ImportProgress {
        stage: ImportStage::Downloading,
        channels_seen: 0,
    });
    match import_content_inner(&db, &cipher, source_id, &content, &token, &listener).await {
        Ok(StagingResult::Completed(outcome)) => listener.on_complete(outcome),
        Ok(StagingResult::Cancelled) => listener.on_failed(ApiError::Cancelled),
        Err(error) => listener.on_failed(error),
    }
}

async fn import_content_inner(
    db: &Arc<Db>,
    cipher: &Arc<CatalogCipher>,
    source_id: SourceId,
    content: &[u8],
    token: &CancelToken,
    listener: &Arc<dyn ImportListener>,
) -> Result<StagingResult, ApiError> {
    if token.is_cancelled() {
        return Ok(StagingResult::Cancelled);
    }
    let (tx, rx) = mpsc::channel::<Vec<u8>>(CHUNK_CHANNEL_DEPTH);
    let worker = {
        let db = Arc::clone(db);
        let cipher = Arc::clone(cipher);
        let token = token.clone();
        let listener = Arc::clone(listener);
        tokio::task::spawn_blocking(move || {
            run_staging(&db, &cipher, source_id, rx, &token, &listener)
        })
    };
    for chunk in content.chunks(CONTENT_CHUNK_SIZE) {
        if token.is_cancelled() {
            break;
        }
        if tx.send(chunk.to_vec()).await.is_err() {
            break; // the worker exited early (error); its result is reported below
        }
    }
    drop(tx); // close the channel so the worker finishes draining

    match worker.await {
        Ok(result) => result,
        Err(join) => {
            warn!(target: targets::IMPORT, cause = %join, "content import staging task failed to join");
            Err(ApiError::Internal)
        }
    }
}

async fn import_inner(
    db: &Arc<Db>,
    cipher: &Arc<CatalogCipher>,
    source_id: SourceId,
    url: &str,
    user_agent: Option<String>,
    accept_invalid_tls: bool,
    token: &CancelToken,
    listener: &Arc<dyn ImportListener>,
) -> Result<StagingResult, ApiError> {
    if token.is_cancelled() {
        return Ok(StagingResult::Cancelled);
    }
    let mut config = FetchConfig {
        accept_invalid_tls,
        ..FetchConfig::default()
    };
    if let Some(agent) = user_agent {
        config.default_user_agent = agent;
    }
    let client = HttpClient::new(&config)?;
    let mut response = client.get(&RequestSpec::new(url)).await?;
    listener.on_progress(ImportProgress {
        stage: ImportStage::Downloading,
        channels_seen: 0,
    });

    let (tx, rx) = mpsc::channel::<Vec<u8>>(CHUNK_CHANNEL_DEPTH);
    let worker = {
        let db = Arc::clone(db);
        let cipher = Arc::clone(cipher);
        let token = token.clone();
        let listener = Arc::clone(listener);
        tokio::task::spawn_blocking(move || {
            run_staging(&db, &cipher, source_id, rx, &token, &listener)
        })
    };

    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| ApiError::from(core_fetch::classify(error)))?
    {
        if token.is_cancelled() {
            break;
        }
        if tx.send(chunk.to_vec()).await.is_err() {
            break; // the worker exited early (error); its result is reported below
        }
    }
    drop(tx); // close the channel so the worker finishes draining

    match worker.await {
        Ok(result) => result,
        Err(join) => {
            warn!(target: targets::IMPORT, cause = %join, "import staging task failed to join");
            Err(ApiError::Internal)
        }
    }
}

/// Blocking side: owns the parser and the staging-and-swap transaction, reporting progress
/// and checking cancellation at each batch boundary.
fn run_staging(
    db: &Db,
    cipher: &CatalogCipher,
    source_id: SourceId,
    mut rx: mpsc::Receiver<Vec<u8>>,
    token: &CancelToken,
    listener: &Arc<dyn ImportListener>,
) -> Result<StagingResult, ApiError> {
    let span = info_span!(target: targets::IMPORT, "import", source = source_id.value());
    let _entered = span.enter();

    // Stages into a throwaway temp-file database on its own connection — no writer lock is held
    // across the streaming `rx.blocking_recv()` loop below, so other writer ops proceed for the
    // download's duration. The single writer is taken only briefly by `commit`, for the swap; see
    // `Db::begin_staging` for the model and memory bound.
    let mut staging = db.begin_staging(source_id)?;
    let mut parser = M3uParser::new();
    let mut sink = ProgressSink {
        staging: &mut staging,
        cipher,
        listener,
        token,
        channels_seen: 0,
        invalid: 0,
        cancelled: false,
    };

    while let Some(chunk) = rx.blocking_recv() {
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
        debug!(target: targets::IMPORT, "import cancelled; discarding the staging database");
        return Ok(StagingResult::Cancelled); // `staging` drops here → temp file gone, main untouched
    }

    let diagnostics = parser.finish(&mut sink).map_err(map_parse_error)?;
    let invalid = sink.invalid;
    let channels_seen = sink.channels_seen;
    // `sink`'s last use ends its `&mut staging` borrow here, so `commit` can take it.
    listener.on_progress(ImportProgress {
        stage: ImportStage::Finalizing,
        channels_seen,
    });
    match staging.commit(db)? {
        RefreshCommit::Committed(outcome) => {
            info!(
                target: targets::IMPORT,
                inserted = outcome.inserted,
                duplicates_dropped = outcome.duplicates_dropped,
                skipped = diagnostics.skipped(),
                "import committed"
            );
            Ok(StagingResult::Completed(ImportOutcome {
                inserted: outcome.inserted,
                duplicates_dropped: outcome.duplicates_dropped,
                emitted: diagnostics.emitted(),
                skipped: diagnostics.skipped(),
                invalid,
            }))
        }
        // The source was deleted while we staged off-lock: the swap was abandoned with the prior
        // (now-cascaded) catalog untouched. Surface it as a cancellation, distinct in the log from
        // a user-driven cancel above.
        RefreshCommit::SourceRemoved => {
            info!(target: targets::IMPORT, "source removed during refresh; prior catalog left intact");
            Ok(StagingResult::Cancelled)
        }
    }
}

/// A [`ChannelSink`] that maps parsed channels to domain rows, stages them, and pushes a
/// progress update per batch. Checks cancellation before staging each batch.
struct ProgressSink<'a, 'l> {
    staging: &'a mut Staging,
    cipher: &'l CatalogCipher,
    listener: &'l Arc<dyn ImportListener>,
    token: &'l CancelToken,
    channels_seen: u64,
    invalid: u64,
    cancelled: bool,
}

impl ChannelSink for ProgressSink<'_, '_> {
    type Error = ApiError;

    fn accept(&mut self, batch: Vec<ParsedChannel>) -> Result<(), Self::Error> {
        if self.cancelled || self.token.is_cancelled() {
            self.cancelled = true;
            return Ok(()); // stop staging at this batch boundary; the caller breaks after `push`
        }
        let mut mapped = Vec::with_capacity(batch.len());
        for parsed in batch {
            let identity = self
                .cipher
                .m3u_identity(parsed.tvg_id(), &parsed.url, &parsed.name)?;
            match map_parsed(parsed) {
                Some(mut channel) => {
                    channel.identity = identity;
                    channel.locator = self.cipher.seal_locator(&channel.locator)?;
                    if let Some(user_agent) = &mut channel.overrides.user_agent {
                        *user_agent = self.cipher.seal_value(user_agent)?;
                    }
                    for (_, value) in &mut channel.overrides.headers {
                        *value = self.cipher.seal_value(value)?;
                    }
                    mapped.push(channel);
                }
                None => self.invalid += 1, // invalid locator: skip-and-count, never fail (§4.2)
            }
        }
        self.staging.stage(&mapped).map_err(ApiError::from)?;
        self.channels_seen += mapped.len() as u64;
        self.listener.on_progress(ImportProgress {
            stage: ImportStage::Downloading,
            channels_seen: self.channels_seen,
        });
        Ok(())
    }
}

/// Maps a raw parsed channel to a domain import row, deriving the deterministic fixture identity
/// and validating the locator (parse, don't validate). Returns `None` if the URL is not a valid
/// locator. Shared with `xtask`'s Phase 1 verifier; the real M3U import sink replaces this public
/// hash with a catalog-keyed identity before persistence.
#[must_use]
pub fn map_parsed(parsed: ParsedChannel) -> Option<NewChannel> {
    let locator = StreamLocator::parse(&parsed.url).ok()?;
    let identity = channel_identity(parsed.tvg_id(), &parsed.url, &parsed.name);
    let epg_key = parsed.tvg_id().map(str::to_owned);
    let group_title = parsed.group().map(str::to_owned);
    let logo = parsed.logo().map(str::to_owned);
    let ParsedChannel {
        name,
        user_agent,
        headers,
        ..
    } = parsed;
    Some(NewChannel {
        identity,
        epg_key,
        name,
        group_title,
        logo,
        locator,
        kind: MediaKind::Live,
        category: None,
        overrides: ChannelOverrides {
            user_agent,
            headers,
            preferred_engine: None,
        },
    })
}

/// Maps the parser's only propagating failure (a sink/DB error) onto the FFI taxonomy.
fn map_parse_error(error: ParseError<ApiError>) -> ApiError {
    match error {
        ParseError::Sink(error) => error,
    }
}
