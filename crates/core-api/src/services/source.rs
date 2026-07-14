// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! `SourceService`: add, list, refresh (with progress), rename, disable, delete
//! (TECH_SPEC §4.6). All three source kinds live here — M3U by URL, M3U from a file, and
//! Xtream — because they are one concept (a configured source) with three payloads, not three
//! services; the FFI service list in §4.6 names no `XtreamService` for that reason.
//!
//! The Xtream password never lives in this service. [`SourceService::add_xtream`] hands it
//! straight to the host store under an opaque key and persists only that key; every later use
//! reads it back through the same callback (TECH_SPEC §12). The key's lifetime is tied to the
//! source: minted on add, deleted on delete, so a removed account leaves nothing behind in the
//! platform keychain.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use core_db::{Db, repo};
use core_model::ids::{SecretRef, SourceId};
use core_model::locator::StreamLocator;
use core_model::secret::Secret;
use core_model::source::{Source as DomainSource, SourceCommon as DomainCommon};
use core_xtream::Endpoint;
use rand::Rng;
use tracing::warn;

use crate::error::ApiError;
use crate::events::{CancelToken, ImportListener, TaskHandle};
use crate::import::{run_import, run_import_content};
use crate::logging::targets;
use crate::records::Source;
use crate::runtime::CoreRuntime;
use crate::secrets::SecretStore;
use crate::xtream::{self, XtreamSource};

/// Cancellation tokens for in-flight long operations (refresh, content import), keyed by source
/// id then by a unique per-operation sequence. Nesting keeps concurrent operations on the same
/// source distinct, so [`SourceService::delete`] can abort *every* running operation for a source
/// it is about to remove while each operation deregisters only its own token.
type RefreshRegistry = Mutex<HashMap<i64, HashMap<u64, CancelToken>>>;

/// Manages configured sources and their catalog refresh.
#[derive(uniffi::Object)]
pub struct SourceService {
    rt: Arc<CoreRuntime>,
    db: Arc<Db>,
    secrets: Arc<dyn SecretStore>,
    refreshes: Arc<RefreshRegistry>,
    /// Monotonic source of the unique per-operation keys used within [`Self::refreshes`].
    next_refresh_seq: AtomicU64,
}

impl SourceService {
    /// Builds the service over shared runtime, database, and host-secrets handles.
    pub(crate) fn new(
        rt: Arc<CoreRuntime>,
        db: Arc<Db>,
        secrets: Arc<dyn SecretStore>,
    ) -> Arc<Self> {
        Arc::new(Self {
            rt,
            db,
            secrets,
            refreshes: Arc::new(Mutex::new(HashMap::new())),
            next_refresh_seq: AtomicU64::new(0),
        })
    }

    /// Registers `token` for an in-flight operation on source `id`, returning its unique key.
    /// A concurrent `delete(id)` cancels *every* registered operation for the source; each
    /// operation deregisters only its own key on completion.
    fn register(&self, id: i64, token: CancelToken) -> u64 {
        let seq = self.next_refresh_seq.fetch_add(1, Ordering::Relaxed);
        self.refreshes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(id)
            .or_default()
            .insert(seq, token);
        seq
    }

    /// Deregisters a completed operation, pruning the id's bucket once it holds no more in-flight
    /// operations so the registry stays bounded by the concurrently-active set.
    fn deregister(refreshes: &RefreshRegistry, id: i64, seq: u64) {
        let mut guard = refreshes.lock().unwrap_or_else(PoisonError::into_inner);
        let bucket_empty = guard.get_mut(&id).is_some_and(|tokens| {
            tokens.remove(&seq);
            tokens.is_empty()
        });
        if bucket_empty {
            guard.remove(&id);
        }
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

    /// Adds an M3U-from-file source (no import yet — call [`Self::import_m3u_content`] with the
    /// picked/pasted playlist text to fill its catalog). File sources have no URL, so they are
    /// import-once: re-importing means calling [`Self::import_m3u_content`] again, never
    /// [`Self::refresh`].
    ///
    /// # Errors
    /// Returns [`ApiError::StorageCorrupt`] if the source cannot be persisted.
    // UniFFI lifts the foreign string into an owned `String`.
    #[allow(clippy::needless_pass_by_value)]
    pub async fn add_m3u_file(&self, name: String) -> Result<Source, ApiError> {
        let db = Arc::clone(&self.db);
        self.rt
            .run_blocking(move || {
                let source = DomainSource::M3uFile {
                    id: SourceId::new(0), // the DB mints the rowid
                    common: DomainCommon {
                        name,
                        enabled: true,
                        auto_refresh_secs: None,
                    },
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

    /// Adds an Xtream Codes account (no import yet — call [`Self::refresh`] to fetch its
    /// catalog).
    ///
    /// The account is **verified before it is stored**: a wrong password should be a sentence
    /// on the add screen, not a mystery on the next refresh. The password is then written to the
    /// host secure store under a freshly minted opaque key and dropped; what reaches SQLite is
    /// the key, never the credential (TECH_SPEC §12).
    ///
    /// # Errors
    /// Returns [`ApiError::InvalidInput`] if `server` is not a valid absolute URL,
    /// [`ApiError::Unauthorized`] if the headend rejects the account,
    /// [`ApiError::NetworkUnreachable`] / [`ApiError::Timeout`] if it cannot be reached, and
    /// [`ApiError::StorageCorrupt`] if the source cannot be persisted.
    pub async fn add_xtream(
        &self,
        name: String,
        server: String,
        username: String,
        password: String,
    ) -> Result<Source, ApiError> {
        let locator = StreamLocator::parse(&server)?; // parse, don't validate
        // Move the caller's `String` into a `Secret` immediately: from here on the credential
        // has a redacted `Debug` and is zeroized on drop, so no later refactor can log it.
        let password = Secret::new(password);
        let endpoint = Endpoint::new(&locator, &username).map_err(xtream::map_error)?;
        let http =
            core_fetch::HttpClient::new(&core_fetch::FetchConfig::default()).map_err(|error| {
                warn!(target: targets::FETCH, %error, "could not build the Xtream client");
                ApiError::Internal
            })?;
        core_xtream::authenticate(&http, &endpoint, &password)
            .await
            .map_err(xtream::map_error)?;

        let secret_ref = mint_secret_ref();
        // Store the credential before the row that references it: a source whose key resolves to
        // nothing is a broken source, whereas an orphaned secret with no source is merely
        // garbage — and `delete` cleans those up anyway. Fail this and nothing was persisted.
        let secrets = Arc::clone(&self.secrets);
        let key = secret_ref.as_str().to_owned();
        let stored = password.expose().to_owned();
        self.rt
            .run_blocking(move || secrets.set(key, stored))
            .await?;
        drop(password);

        let db = Arc::clone(&self.db);
        self.rt
            .run_blocking(move || {
                let source = DomainSource::Xtream {
                    id: SourceId::new(0), // the DB mints the rowid
                    common: DomainCommon {
                        name,
                        enabled: true,
                        auto_refresh_secs: None,
                    },
                    server: locator,
                    username,
                    secret: secret_ref,
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

    /// The playable URL for a channel's stored locator — call this immediately before handing a
    /// stream to an engine.
    ///
    /// **Kind-agnostic by design.** An M3U locator is already playable and comes back unchanged;
    /// an Xtream locator is stored credential-free (§12, `core_xtream::urls`) and gets its
    /// credentials put back here, read from the host store. The shell therefore does not need to
    /// know which kind of source a channel came from — it asks for a playable URL and gets one,
    /// which is what keeps the zap path (PRD §8.4) free of per-kind branching.
    ///
    /// The returned string carries credentials for an Xtream source. It is bound for the engine
    /// and nowhere else: it must not be logged, persisted, or held past the play call. Resolve
    /// per play rather than caching — the whole point of storing a credential-free catalog is
    /// that the playable form does not outlive its use.
    ///
    /// # Errors
    /// Returns [`ApiError::NotFound`] if the source is gone or its stored locator is not a
    /// recognizable reference (a stale row; the source needs a refresh), [`ApiError::Unauthorized`]
    /// if the account's password is missing from the host store, [`ApiError::InvalidInput`] if
    /// `locator` is not a valid address, and [`ApiError::StorageCorrupt`] on a read failure.
    pub async fn resolve_stream(
        &self,
        source_id: i64,
        locator: String,
    ) -> Result<String, ApiError> {
        let parsed = StreamLocator::parse(&locator)?; // parse, don't validate
        let db = Arc::clone(&self.db);
        let source = self
            .rt
            .run_blocking(move || read_source(&db, SourceId::new(source_id)))
            .await?
            .ok_or(ApiError::NotFound)?;
        match source {
            // Already playable: an M3U playlist's URLs are what the playlist said they were.
            DomainSource::M3uUrl { .. } | DomainSource::M3uFile { .. } => Ok(locator),
            DomainSource::Xtream {
                server,
                username,
                secret,
                ..
            } => {
                xtream::resolve_playable(
                    &XtreamSource {
                        id: SourceId::new(source_id),
                        server,
                        username,
                        secret,
                    },
                    &self.secrets,
                    &parsed,
                )
                .await
            }
        }
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

    /// Deletes a source and (by cascade) its catalog, favorites, hidden flags, and history —
    /// and, for an Xtream account, its password in the host secure store.
    ///
    /// Deleting the credential is part of deleting the source, not a nicety: the DB row is the
    /// only record of which opaque key belongs to this account, so a delete that skipped it
    /// would strand the password in the platform keychain with nothing left able to name it
    /// (TECH_SPEC §12).
    ///
    /// Signals every in-flight refresh for this source to cancel first, so a still-downloading
    /// import aborts at its next batch boundary and discards its staged catalog rather than
    /// swapping one in for a source that is about to vanish. This is best-effort: a refresh already
    /// past its last boundary is caught instead by the commit-time existence check, which abandons
    /// the swap and reports the refresh as cancelled — never a spurious storage failure.
    ///
    /// # Errors
    /// Returns [`ApiError::StorageCorrupt`] on a write failure.
    pub async fn delete(&self, id: i64) -> Result<(), ApiError> {
        // Signal cancellation before contending for the writer: whether the delete or a
        // refresh's staging transaction wins the writer mutex, each refresh observes the flag at
        // its next boundary and rolls back cleanly rather than surfacing a spurious failure.
        // Take the id's whole bucket so *all* of its concurrent refreshes are cancelled, not
        // just whichever registered most recently.
        let active = self
            .refreshes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&id);
        if let Some(tokens) = active {
            for token in tokens.into_values() {
                token.cancel();
            }
        }
        let db = Arc::clone(&self.db);
        let secrets = Arc::clone(&self.secrets);
        self.rt
            .run_blocking(move || {
                // Read the source before removing it: its row carries the only pointer to the
                // credential, so the key must be recovered while it still exists.
                let secret = {
                    let conn = db.reader()?;
                    match repo::sources::get(&conn, SourceId::new(id))? {
                        Some(DomainSource::Xtream { secret, .. }) => Some(secret),
                        _ => None,
                    }
                };
                {
                    let conn = db.writer();
                    repo::sources::delete(&conn, SourceId::new(id))?;
                }
                if let Some(secret) = secret {
                    // The row is already gone, so a failing store must not fail the delete —
                    // the user asked for the source to be removed and it has been. Log it: a
                    // stranded key is a real (if minor) hygiene problem worth seeing.
                    if let Err(error) = secrets.delete(secret.as_str().to_owned()) {
                        warn!(
                            target: targets::DB,
                            %error,
                            "source deleted, but its stored credential could not be removed"
                        );
                    }
                }
                Ok(())
            })
            .await
    }

    /// Refreshes a source's catalog from its URL. Returns immediately with a [`TaskHandle`];
    /// progress, completion, and failure arrive on `listener`. The download stages off-lock into a
    /// throwaway database and swaps into the live catalog only at the end, so cancellation via the
    /// handle — checked at batch boundaries — leaves the prior catalog intact on abort, and other
    /// writes are never blocked for the download's duration.
    #[must_use]
    pub fn refresh(&self, id: i64, listener: Box<dyn ImportListener>) -> Arc<TaskHandle> {
        let token = CancelToken::default();
        let db = Arc::clone(&self.db);
        let secrets = Arc::clone(&self.secrets);
        let listener: Arc<dyn ImportListener> = Arc::from(listener);
        let task_token = token.clone();
        let seq = self.register(id, token.clone());
        let refreshes = Arc::clone(&self.refreshes);
        self.rt.spawn(async move {
            run_refresh(db, SourceId::new(id), secrets, task_token, listener).await;
            Self::deregister(&refreshes, id, seq);
        });
        Arc::new(TaskHandle::new(token))
    }

    /// Imports an M3U-from-file source's catalog from in-memory `content` (the picked/SAF/pasted
    /// playlist text). Returns immediately with a [`TaskHandle`]; progress, completion, and
    /// failure arrive on `listener`, exactly like [`Self::refresh`]. The content is staged and
    /// swapped atomically — a cancellation (via the handle or a concurrent `delete`, checked at
    /// batch boundaries) leaves any prior catalog intact.
    #[must_use]
    pub fn import_m3u_content(
        &self,
        id: i64,
        content: String,
        listener: Box<dyn ImportListener>,
    ) -> Arc<TaskHandle> {
        let token = CancelToken::default();
        let db = Arc::clone(&self.db);
        let listener: Arc<dyn ImportListener> = Arc::from(listener);
        let task_token = token.clone();
        let seq = self.register(id, token.clone());
        let refreshes = Arc::clone(&self.refreshes);
        self.rt.spawn(async move {
            run_import_content(
                db,
                SourceId::new(id),
                content.into_bytes(),
                task_token,
                listener,
            )
            .await;
            Self::deregister(&refreshes, id, seq);
        });
        Arc::new(TaskHandle::new(token))
    }
}

/// Reads a source's definition (on the runtime), then drives the import its kind calls for.
///
/// The dispatch is exhaustive over [`DomainSource`], so adding a fourth source kind is a
/// compile error here rather than a silent "nothing happened" at runtime.
async fn run_refresh(
    db: Arc<Db>,
    source_id: SourceId,
    secrets: Arc<dyn SecretStore>,
    token: CancelToken,
    listener: Arc<dyn ImportListener>,
) {
    let read = {
        let db = Arc::clone(&db);
        tokio::task::spawn_blocking(move || read_source(&db, source_id)).await
    };
    let source = match read {
        Ok(Ok(Some(source))) => source,
        Ok(Ok(None)) => return listener.on_failed(ApiError::NotFound),
        Ok(Err(error)) => return listener.on_failed(error),
        Err(_) => return listener.on_failed(ApiError::Internal),
    };
    match source {
        DomainSource::M3uUrl {
            url,
            user_agent,
            accept_invalid_tls,
            ..
        } => {
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
        DomainSource::Xtream {
            server,
            username,
            secret,
            ..
        } => {
            xtream::run_refresh(
                db,
                XtreamSource {
                    id: source_id,
                    server,
                    username,
                    secret,
                },
                secrets,
                token,
                listener,
            )
            .await;
        }
        // A file source has no address to fetch from: re-importing means handing the picked or
        // pasted text back through `import_m3u_content`. Not a failure of this refresh so much
        // as the wrong call, so it says which call to make instead (PRD §6.3).
        DomainSource::M3uFile { .. } => listener.on_failed(ApiError::InvalidInput {
            reason: "this source is imported from a file — pick the file again to update it"
                .to_owned(),
        }),
    }
}

/// Mints a fresh opaque host-secrets key.
///
/// Random rather than derived from the source id, for two reasons: the id is not known until
/// the row is inserted (and the row must carry the key), and "opaque" should mean it — a key an
/// attacker can compute from a rowid tells them what to ask the keychain for. 128 bits of
/// hex from the OS CSPRNG, namespaced so a keychain browser shows what it belongs to.
fn mint_secret_ref() -> SecretRef {
    let bytes: [u8; 16] = rand::rng().random();
    let mut key = String::with_capacity("xtream/".len() + bytes.len() * 2);
    key.push_str("xtream/");
    for byte in bytes {
        // Writing into a `String` is infallible — its `fmt::Write` impl cannot fail — so the
        // `Result` carries no information and is deliberately dropped rather than unwrapped.
        let _ = write!(key, "{byte:02x}");
    }
    SecretRef::new(key)
}

/// Blocking read of one source's persisted definition.
fn read_source(db: &Db, id: SourceId) -> Result<Option<DomainSource>, ApiError> {
    let conn = db.reader()?;
    Ok(repo::sources::get(&conn, id)?)
}
