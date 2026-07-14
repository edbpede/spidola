// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The Xtream catalog import pipeline (handshake → listings → staging-and-swap), owned by
//! `SourceService` (TECH_SPEC §4.6, §4.3).
//!
//! A peer of [`crate::import`], not a branch of it: the two share a destination (the
//! staging-and-swap transaction) and nothing else. M3U is one streamed body parsed line by
//! line; Xtream is a handshake followed by a series of JSON listings, each whole before it can
//! be read (`core_xtream::request`'s module docs explain why the protocol leaves no choice).
//! Folding one into the other would produce a function shaped like neither.
//!
//! What they *do* share is the contract this module keeps identical to the M3U path: progress
//! is reported per stage, cancellation is checked at every batch boundary, and a cancelled or
//! failed import drops its transaction un-committed so the prior catalog survives intact
//! (TECH_SPEC §4.4). Exactly one terminal `listener` call happens on every path.
//!
//! **The password's whole life is inside this module**: read from the host store, held in a
//! [`Secret`], borrowed by `core-xtream` for each request, and dropped before the catalog is
//! written. It is never persisted, never logged, and never reaches the catalog — what lands in
//! the DB is `core-xtream`'s credential-free locator, resolved back into a playable URL only at
//! play time (§12, `core_xtream::urls`).

use std::sync::Arc;

use core_db::{Db, NewChannel, RefreshCommit, Staging};
use core_fetch::{FetchConfig, HttpClient};
use core_model::channel::{ChannelOverrides, MediaKind};
use core_model::ids::{SecretRef, SourceId};
use core_model::locator::StreamLocator;
use core_model::secret::Secret;
use core_xtream::{CatalogChannel, Endpoint, XtreamError, catalog, series};
use tracing::{info, info_span, warn};

use crate::error::ApiError;
use crate::events::{CancelToken, ImportListener, ImportOutcome, ImportProgress, ImportStage};
use crate::logging::targets;
use crate::secrets::SecretStore;

/// Channels staged per transaction batch. Matches the M3U path's cadence, so cancellation
/// granularity and progress reporting feel the same whatever kind of source is refreshing.
const STAGE_BATCH: usize = 500;

/// Everything the import needs about the source, read before the network work begins.
pub(crate) struct XtreamSource {
    /// Persisted identity.
    pub(crate) id: SourceId,
    /// The headend's base URL.
    pub(crate) server: StreamLocator,
    /// Account username.
    pub(crate) username: String,
    /// Opaque host-secrets key naming the password.
    pub(crate) secret: SecretRef,
}

/// Runs a full Xtream catalog refresh, reporting to `listener` and honouring `token`.
///
/// Intended to be driven as a task on the core runtime; it never panics across the boundary —
/// every failure path ends in exactly one terminal `listener` call.
pub(crate) async fn run_refresh(
    db: Arc<Db>,
    source: XtreamSource,
    secrets: Arc<dyn SecretStore>,
    token: CancelToken,
    listener: Arc<dyn ImportListener>,
) {
    listener.on_progress(ImportProgress {
        stage: ImportStage::Connecting,
        channels_seen: 0,
    });
    match refresh_inner(&db, &source, &secrets, &token, &listener).await {
        Ok(Some(outcome)) => listener.on_complete(outcome),
        Ok(None) => listener.on_failed(ApiError::Cancelled),
        Err(error) => listener.on_failed(error),
    }
}

/// The import proper. `Ok(None)` means "cancelled at a batch boundary".
async fn refresh_inner(
    db: &Arc<Db>,
    source: &XtreamSource,
    secrets: &Arc<dyn SecretStore>,
    token: &CancelToken,
    listener: &Arc<dyn ImportListener>,
) -> Result<Option<ImportOutcome>, ApiError> {
    let span = info_span!(target: targets::IMPORT, "xtream_refresh");
    let _entered = span.enter();

    let password = load_password(secrets, &source.secret).await?;
    let endpoint = Endpoint::new(&source.server, &source.username).map_err(map_error)?;
    let http = HttpClient::new(&FetchConfig::default()).map_err(|error| {
        warn!(target: targets::FETCH, %error, "could not build the Xtream client");
        ApiError::Internal
    })?;

    // Authenticate before touching the catalog: a wrong or lapsed account should surface as
    // `Unauthorized` in a second, not after a 20 MB download.
    let status = core_xtream::authenticate(&http, &endpoint, &password)
        .await
        .map_err(map_error)?;
    info!(
        target: targets::XTREAM,
        max_connections = status.max_connections,
        "xtream account accepted"
    );

    let Some(collected) = collect(&http, &endpoint, &password, token, listener).await? else {
        return Ok(None);
    };
    // The password has no further use; drop it before the staging work rather than keeping
    // credential material alive across it.
    drop(password);

    listener.on_progress(ImportProgress {
        stage: ImportStage::Finalizing,
        channels_seen: collected.emitted,
    });
    stage_and_swap(db, source.id, collected, token).await
}

/// Every listing this refresh fetched, flattened into rows plus the accounting.
///
/// `core-xtream` keeps a `Diagnostics` ledger per listing and offers no way to merge two (its
/// counters are private, by design — a ledger describes one mapping pass). The totals are
/// summed here instead, which is all the boundary's [`ImportOutcome`] reports.
struct Collected {
    channels: Vec<CatalogChannel>,
    emitted: u64,
    skipped: u64,
}

/// Fetches the live, VOD, and series catalogs. `Ok(None)` on cancellation.
async fn collect(
    http: &HttpClient,
    endpoint: &Endpoint,
    password: &Secret,
    token: &CancelToken,
    listener: &Arc<dyn ImportListener>,
) -> Result<Option<Collected>, ApiError> {
    let mut collected = Collected {
        channels: Vec::new(),
        emitted: 0,
        skipped: 0,
    };

    // Live and VOD share a shape: categories supply the group labels, then one listing call.
    for kind in [MediaKind::Live, MediaKind::Movie] {
        if token.is_cancelled() {
            return Ok(None);
        }
        listener.on_progress(ImportProgress {
            stage: ImportStage::Downloading,
            channels_seen: collected.emitted,
        });
        let categories = catalog::categories(http, endpoint, password, kind)
            .await
            .map_err(map_error)?;
        // Awaited inside each arm rather than on the `match`: the two calls return distinct
        // opaque futures, so the arms have no common type until they are awaited.
        let listing = match kind {
            MediaKind::Live => {
                catalog::live_streams(http, endpoint, password, &categories, None).await
            }
            _ => catalog::vod_streams(http, endpoint, password, &categories, None).await,
        }
        .map_err(map_error)?;
        collected.emitted += listing.diagnostics.emitted();
        collected.skipped += listing.diagnostics.skipped();
        collected.channels.extend(listing.channels);
    }

    if token.is_cancelled() {
        return Ok(None);
    }

    // Series are two-level: list the shows, then one info call per show to reach its episodes.
    // Only episodes are playable, so only they become channels — a show is a grouping, and
    // `core-xtream` already writes the show's name into each episode's group label, which is
    // what makes the browse axis read source → type → series → episode without a join here.
    let shows = series::list(http, endpoint, password, None)
        .await
        .map_err(map_error)?;
    for show in &shows {
        if token.is_cancelled() {
            return Ok(None);
        }
        match series::expand(http, endpoint, password, show.series_id).await {
            Ok(expansion) => {
                collected.emitted += expansion.diagnostics.emitted();
                collected.skipped += expansion.diagnostics.skipped();
                collected.channels.extend(
                    expansion
                        .episodes
                        .into_iter()
                        .map(|episode| episode.channel),
                );
                listener.on_progress(ImportProgress {
                    stage: ImportStage::Downloading,
                    channels_seen: collected.emitted,
                });
            }
            Err(error) => {
                // One unreachable show must not cost the user their whole catalog — the same
                // skip-and-count posture the row mappers take, one level up.
                warn!(target: targets::XTREAM, %error, "skipping a series that would not expand");
                collected.skipped += 1;
            }
        }
    }

    Ok(Some(collected))
}

/// Stages the collected rows and swaps them in atomically. `Ok(None)` on cancellation.
async fn stage_and_swap(
    db: &Arc<Db>,
    source_id: SourceId,
    collected: Collected,
    token: &CancelToken,
) -> Result<Option<ImportOutcome>, ApiError> {
    let db = Arc::clone(db);
    let token = token.clone();
    tokio::task::spawn_blocking(move || {
        let mut staging: Staging = db.begin_staging(source_id)?;
        for batch in collected.channels.chunks(STAGE_BATCH) {
            // Honest cancellation, checked at every batch boundary: a departing screen stops
            // this within one batch and the transaction drops un-committed, so the prior
            // catalog is never touched (TECH_SPEC §4.4).
            if token.is_cancelled() {
                return Ok(None);
            }
            let rows: Vec<NewChannel> = batch.iter().map(to_new_channel).collect();
            staging.stage(&rows)?;
        }
        if token.is_cancelled() {
            return Ok(None);
        }
        match staging.commit(&db)? {
            RefreshCommit::Committed(outcome) => {
                info!(
                    target: targets::IMPORT,
                    inserted = outcome.inserted,
                    skipped = collected.skipped,
                    "xtream catalog swapped in"
                );
                Ok(Some(ImportOutcome {
                    inserted: outcome.inserted,
                    duplicates_dropped: outcome.duplicates_dropped,
                    emitted: collected.emitted,
                    skipped: collected.skipped,
                    invalid: 0,
                }))
            }
            // The source was deleted while we fetched off-lock: the swap was abandoned with
            // nothing written. Surfaced as a cancellation, never a storage error.
            RefreshCommit::SourceRemoved => Ok(None),
        }
    })
    .await
    .map_err(|_| ApiError::Internal)?
}

/// Maps one Xtream catalog row onto a storable channel.
///
/// `category` stays `None` and the group label rides in `group_title`, exactly as the M3U path
/// does: browse groups are derived from `group_title`, so resolving a `CategoryId` here would
/// buy nothing and would put two sources' rows into two shapes.
fn to_new_channel(channel: &CatalogChannel) -> NewChannel {
    NewChannel {
        identity: channel.identity,
        name: channel.name.clone(),
        group_title: channel.group_title.clone(),
        logo: channel.logo.clone(),
        locator: channel.locator.clone(),
        kind: channel.kind,
        category: None,
        // Xtream carries its auth in the URL, not in headers, so it needs no per-channel
        // override. A user's engine preference is applied on top of this by the shell.
        overrides: ChannelOverrides {
            user_agent: None,
            headers: Vec::new(),
            preferred_engine: None,
        },
    }
}

/// Reads the account password from the host store.
///
/// The store is the shell's Keychain / Keystore, so the call can block; it runs on the blocking
/// pool rather than an async worker (the standing rule, TECH_SPEC §4.6).
async fn load_password(
    secrets: &Arc<dyn SecretStore>,
    key: &SecretRef,
) -> Result<Secret, ApiError> {
    let secrets = Arc::clone(secrets);
    let key = key.as_str().to_owned();
    let value = tokio::task::spawn_blocking(move || secrets.get(key))
        .await
        .map_err(|_| ApiError::Internal)??;
    // A source whose secret vanished from the host store cannot be repaired by retrying; the
    // user must re-enter the password, which is exactly what `Unauthorized` prescribes
    // (PRD §6.3).
    value.map(Secret::new).ok_or(ApiError::Unauthorized)
}

/// Flattens `core-xtream`'s taxonomy into the boundary's (TECH_SPEC §4.7).
///
/// The detail goes to the log stream, never into the FFI error — user-facing errors stay
/// jargon-free (PRD §8.6) while diagnostics stay rich. Every `XtreamError`'s `Display` is
/// written to be safe to log: none of them echo a response payload, which matters because
/// Xtream's `user_info` mirrors the account password back (`core_xtream::error`).
pub(crate) fn map_error(error: XtreamError) -> ApiError {
    warn!(target: targets::XTREAM, error = %error, "an xtream call failed");
    match error {
        // Every rejection flavour lands on `Unauthorized`: each is fixed by the user correcting
        // or renewing their account, which is the action that variant prescribes. Which flavour
        // it was is in the line above, for a support thread rather than the screen.
        XtreamError::Unauthorized { .. } => ApiError::Unauthorized,
        XtreamError::Transport(fetch) => ApiError::from(fetch),
        XtreamError::Malformed { .. } | XtreamError::ResponseTooLarge { .. } => {
            ApiError::ParseFailed {
                emitted: 0,
                skipped: 0,
            }
        }
        XtreamError::InvalidServer { .. } => ApiError::InvalidInput {
            reason: "that server address isn't valid".to_owned(),
        },
    }
}
