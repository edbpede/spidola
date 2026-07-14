// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! `PairingService`: start/stop the LAN server, report its URL, surface submissions as events
//! (TECH_SPEC §4.6, §12; PRD §6.1).
//!
//! Thin by design. `core-pair` owns the server, the token, and the form; this service owns only
//! the two things that crate deliberately does not know about: the FFI (it adapts `core-pair`'s
//! [`SubmissionSink`] onto a UniFFI callback interface) and the server's lifetime (it holds the
//! one live [`PairServer`], so "alive only while its screen is visible" becomes a `start`/`stop`
//! pair the shell drives from its screen lifecycle).
//!
//! **Why the submitted password crosses the boundary.** A phone's Xtream submission arrives here
//! with its password in a `Secret`, and [`PairingSubmission`] hands it to the shell in the clear.
//! That is deliberate: PRD §6.1 says a submission "lands as a pre-filled add-source flow" — the
//! person at the TV still confirms it — so the password must reach the same add screen a typed
//! one would, and leave by the same door,
//! [`SourceService::add_xtream`](crate::services::SourceService::add_xtream). Keeping it core-side
//! behind a handle would add a stateful pending-submission registry to avoid an exposure the
//! manual path already has (that call takes a password across the boundary too), and would still
//! end at the same call. §12's rule is about where credentials *rest* — the host secure store,
//! never SQLite, never a log — and that is unchanged: this one is in flight to that store, and
//! nothing here writes it anywhere.

use std::net::Ipv4Addr;
use std::sync::{Arc, Mutex, PoisonError};

use core_pair::{PairError, PairServer, Submission, SubmissionSink};
use tracing::{info, warn};

use crate::error::ApiError;
use crate::logging::targets;
use crate::runtime::CoreRuntime;

/// What the pairing screen renders while the server is up.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PairingSession {
    /// The address to show and encode as a QR code, e.g. `http://192.168.1.40:53219`.
    ///
    /// Guaranteed to be an address the server would answer on: `core-pair` checks the
    /// advertised host against the same predicate as its peer check, so a URL that exists is
    /// one a phone on the LAN can dial.
    pub url: String,
    /// The bound port. Reported so a shell can recognize its own session, and because a URL is
    /// only ever host + port.
    pub port: u16,
    /// This session's token, for the person reading the screen to type on their phone.
    pub token: String,
}

/// What a phone submitted, ready to pre-fill the TV's add-source flow.
///
/// Mirrors `core_pair::Submission` as flat owned data (TECH_SPEC §5). The URLs arrive parsed and
/// are flattened back to strings because that is what `SourceService`'s add methods take — the
/// shell passes them straight through.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum PairingSubmission {
    /// An M3U/M3U8 playlist to fetch by URL.
    M3uUrl {
        /// The playlist URL.
        url: String,
    },
    /// An Xtream Codes account.
    Xtream {
        /// The Xtream server base URL.
        server: String,
        /// The account username.
        username: String,
        /// The account password, in flight to the host secure store via
        /// `SourceService::add_xtream`. The shell hands it to that call and keeps it nowhere
        /// else — not in a log, not in its own storage (TECH_SPEC §12).
        password: String,
    },
}

impl From<Submission> for PairingSubmission {
    fn from(submission: Submission) -> Self {
        match submission {
            Submission::M3uUrl { url } => Self::M3uUrl {
                url: url.to_string(),
            },
            Submission::Xtream {
                server,
                username,
                password,
            } => Self::Xtream {
                server: server.to_string(),
                username,
                // The one sanctioned exposure, per this module's header. An explicit, greppable
                // act rather than an implicit conversion — which is exactly why `Secret` has no
                // `Display` or `Into<String>`.
                password: password.expose().to_owned(),
            },
        }
    }
}

/// Receives what the phone sent.
///
/// **Threading contract:** invoked from the pairing server's connection task — it may arrive on
/// *any* core thread, and the shell must trampoline to its own main actor/dispatcher (TECH_SPEC
/// §5). It must not block: the phone is waiting on a response behind it.
#[uniffi::export(callback_interface)]
pub trait PairingListener: Send + Sync {
    /// Called at most once per accepted submission.
    fn on_submission(&self, submission: PairingSubmission);
}

/// Adapts `core-pair`'s sink onto the FFI listener — the whole reason this service exists.
/// `core-pair` depends on nothing above `core-model` and must not learn that UniFFI is here.
struct ListenerSink {
    listener: Arc<dyn PairingListener>,
}

impl SubmissionSink for ListenerSink {
    fn submit(&self, submission: Submission) {
        self.listener
            .on_submission(PairingSubmission::from(submission));
    }
}

/// Starts and stops the LAN pairing server.
#[derive(uniffi::Object)]
pub struct PairingService {
    rt: Arc<CoreRuntime>,
    /// The one live server, or `None` while the pairing screen is closed.
    ///
    /// A `Mutex` because `stop` must *take* the server (its `stop` consumes `self`) and two
    /// screens racing to start must not each end up with one. The guard is never held across an
    /// `.await` — it is taken, the value moved out, and the guard dropped before any awaiting.
    server: Mutex<Option<PairServer>>,
}

impl PairingService {
    /// Builds the service over the shared runtime handle.
    pub(crate) fn new(rt: Arc<CoreRuntime>) -> Arc<Self> {
        Arc::new(Self {
            rt,
            server: Mutex::new(None),
        })
    }
}

#[uniffi::export]
impl PairingService {
    /// Starts the server and returns what the pairing screen should render.
    ///
    /// `host` is the TV's LAN address to advertise. **A shell should supply it**: it can
    /// enumerate its own interfaces (`NWInterface` on tvOS, `WifiManager` / `NetworkInterface`
    /// on Android) and the core cannot — `core-pair` infers the address from the route out of
    /// the host, which is right on a plain LAN and wrong behind a full-tunnel VPN or on any
    /// multi-homed device (its docs carry the measurements). `None` asks for that inference as
    /// the convenience path, and fails loudly rather than advertising an address that will not
    /// answer.
    ///
    /// Starting while one already runs stops the old server first, so a re-entered screen gets a
    /// fresh token rather than silently reusing the last one — a token's whole meaning is
    /// "someone is looking at this screen right now", and a stale one outlives that claim.
    ///
    /// # Errors
    /// Returns [`ApiError::InvalidInput`] if `host` is not a usable LAN address (either supplied
    /// as one, or inferred as one — see the note on that variant's message), or
    /// [`ApiError::Internal`] if the socket cannot be opened.
    pub async fn start(
        &self,
        host: Option<String>,
        listener: Box<dyn PairingListener>,
    ) -> Result<PairingSession, ApiError> {
        let host = match host {
            Some(host) => Some(host.parse::<Ipv4Addr>().map_err(|_| {
                warn!(target: targets::PAIR, "the supplied pairing address is not an IPv4 address");
                ApiError::InvalidInput {
                    reason: "that isn't a network address".to_owned(),
                }
            })?),
            None => None,
        };
        self.stop().await;
        let sink: Arc<dyn SubmissionSink> = Arc::new(ListenerSink {
            listener: Arc::from(listener),
        });
        // Spawned onto the core runtime rather than awaited here: the server binds a Tokio
        // listener and spawns its own accept loop, both of which need our reactor to be the
        // ambient one (TECH_SPEC §4.6 — the shells never see the runtime).
        let server = self
            .rt
            .spawn(async move {
                match host {
                    Some(host) => PairServer::start_advertising(host, sink).await,
                    None => PairServer::start(sink).await,
                }
            })
            .await
            .map_err(|_| ApiError::Internal)?
            .map_err(|error| map_error(&error))?;
        let session = PairingSession {
            url: server.url().to_owned(),
            port: server.port(),
            token: server.token().display().to_owned(),
        };
        info!(target: targets::PAIR, port = session.port, "pairing server started");
        *self.server.lock().unwrap_or_else(PoisonError::into_inner) = Some(server);
        Ok(session)
    }

    /// Stops the server, if one is running. Idempotent.
    ///
    /// The shell calls this when the pairing screen goes away. Dropping the server does the same
    /// thing, so a shell that forgets to call this still cannot leave a listener on the LAN —
    /// this exists so the stop is *prompt* and awaited, not so it is possible.
    pub async fn stop(&self) {
        let server = self
            .server
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(server) = server {
            // A join failure means the accept task is already gone, which is the state we were
            // asking for; there is nothing to report and nothing to retry.
            self.rt.spawn(server.stop()).await.ok();
            info!(target: targets::PAIR, "pairing server stopped");
        }
    }
}

/// Flattens `core-pair`'s taxonomy into the boundary's (TECH_SPEC §4.7).
///
/// The address failures are deliberately **not** collapsed into `Internal`. `core-pair`'s docs
/// are emphatic that `NotOnPrivateNetwork` means "we could not work out this TV's address", not
/// "this TV is off the network" — a full-tunnel VPN produces it on a perfectly connected TV — so
/// rendering it as a plumbing fault would tell the user a falsehood and prescribe no action. As
/// `InvalidInput` it carries a plain sentence and points at the one thing that fixes it: the
/// shell supplying the address its platform reports (PRD §6.3 — an error with no action is a
/// design bug).
fn map_error(error: &PairError) -> ApiError {
    warn!(target: targets::PAIR, error = %error, "the pairing server could not start");
    match error {
        PairError::NoLanAddress(_) | PairError::NotOnPrivateNetwork { .. } => {
            ApiError::InvalidInput {
                reason: "we couldn't work out this TV's address on your network".to_owned(),
            }
        }
        PairError::Bind(_) => ApiError::Internal,
    }
}
