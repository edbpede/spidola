// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! LAN-only server lifecycle: binding, routing, and the hardening budget (TECH_SPEC §12).
//!
//! This is the only listening socket Spidola ever opens, and the only code an
//! unauthenticated stranger can reach. It answers two routes, holds no state but a token,
//! and dies with the screen that summoned it.
//!
//! Three properties carry the security claim in §12 — "a person on the network cannot inject
//! a source into a TV they cannot see" — and each is enforced somewhere different:
//!
//! - **Line of sight** is the token ([`crate::token`]): it is only ever rendered on the TV.
//! - **Locality** is the peer check (`is_local`), *not* the bind. The listener binds
//!   `0.0.0.0` on purpose — a TV does not know which interface the phone will arrive on, and
//!   an interface allowlist would be a platform-specific dependency that gets it subtly
//!   wrong. Instead every connection's peer address is judged before a byte is read. This
//!   also means the advertised URL is not load-bearing for security: a phone that finds the
//!   server by some other address is judged the same way as one that used the URL.
//! - **Liveness** is [`PairServer`]'s ownership: it stops on [`PairServer::stop`] *and* on
//!   drop, so "only while its screen is visible" is enforced by the type system rather than
//!   by the shell remembering.
//!
//! Everything else here is the hardening budget. Phase 7 runs hostile input at this surface,
//! so the caps are stated once as constants and applied before any allocation they bound.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{
    AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader,
};
use tokio::net::{TcpListener, UdpSocket};
use tokio::sync::watch;
use tokio::task::{JoinHandle, JoinSet};
use tokio::time::timeout;

use crate::error::{PairError, Rejection, Status};
use crate::form::{self, Fields, Submission};
use crate::token::PairToken;

/// This crate's log target, following the target-per-subsystem convention in TECH_SPEC §4.8
/// (`core_api::logging::targets`). Named here rather than imported because `core-api` sits
/// above this crate and depending on it would invert the layering.
const TARGET: &str = "spidola::pair";

/// How long one client has to deliver a complete request. The defence against slow-loris:
/// a connection that dribbles bytes is holding a task, and this is what bounds that.
const READ_TIMEOUT: Duration = Duration::from_secs(5);
/// How long one response has to be written before the connection is abandoned.
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);
/// Longest accepted request line. Both real request lines are under 20 bytes.
const MAX_REQUEST_LINE: usize = 2048;
/// Most header lines accepted from one client.
const MAX_HEADERS: usize = 32;
/// Most header bytes accepted from one client, across all lines.
const MAX_HEADER_BYTES: usize = 8192;
/// Largest accepted request body. The form's fields cannot plausibly reach this.
const MAX_BODY: usize = 4096;
/// Most connections served at once. A phone needs one, plus a stray favicon probe; the cap
/// exists so a stranger cannot turn "one task per connection" into unbounded memory.
const MAX_CONNECTIONS: usize = 16;

/// Where accepted submissions go.
///
/// A trait rather than a channel so `core-api` can adapt it straight onto a UniFFI callback
/// interface (TECH_SPEC §5) without this crate knowing the FFI exists — `core-pair` depends
/// on nothing above `core-model` and this is what keeps that true.
///
/// **Threading contract**, matching `core_api::logging::LogSink`: [`Self::submit`] is called
/// synchronously from the connection's task and may arrive on any core thread. It must not
/// block — the phone is waiting on a response behind it.
pub trait SubmissionSink: Send + Sync + 'static {
    /// Hands one validated submission to the shell, which pre-fills its add-source flow
    /// (PRD §6.1). Called at most once per accepted POST.
    fn submit(&self, submission: Submission);
}

/// A running pairing server.
///
/// Holding this value is what keeps the server alive: drop it and the listener closes. The
/// pairing screen owns one for exactly as long as it is on screen.
#[derive(Debug)]
pub struct PairServer {
    url: String,
    port: u16,
    token: Arc<PairToken>,
    shutdown: watch::Sender<bool>,
    accept_task: Option<JoinHandle<()>>,
}

impl PairServer {
    /// Starts the server on an ephemeral port, advertising the address this host appears to
    /// reach the network from.
    ///
    /// The convenience entry point, and the one a shell should try first. It is
    /// [`PairServer::start_advertising`] with the address inferred from this host's route to
    /// the outside — see the crate docs for when that inference is wrong and what to do then.
    ///
    /// The token is generated here rather than supplied, so a caller can neither forget it
    /// nor weaken it. Read it back with [`PairServer::token`] and the URL with
    /// [`PairServer::url`]; together they are what the pairing screen renders (as text and as
    /// a QR code — this crate has no opinion on the QR).
    ///
    /// # Errors
    /// [`PairError::Bind`] if the socket cannot be opened; [`PairError::NoLanAddress`] or
    /// [`PairError::NotOnPrivateNetwork`] if no dialable address could be inferred. The last
    /// of those does **not** mean the TV is off the network — read its docs before rendering
    /// it to a user.
    pub async fn start(sink: Arc<dyn SubmissionSink>) -> Result<Self, PairError> {
        Self::start_advertising(private_ipv4().await?, sink).await
    }

    /// Starts the server on an ephemeral port, advertising `host`.
    ///
    /// Public because choosing the advertised interface is a real capability, not a hatch.
    /// This crate cannot reliably discover its own LAN address: [`PairServer::start`] infers
    /// it from the route out, which a full-tunnel VPN or any multi-homed host makes wrong,
    /// and no dependency-free probe fixes that (measurements in the crate docs). The shells
    /// *can*: `NWInterface` on tvOS and `WifiManager` / `NetworkInterface` on Android
    /// enumerate interfaces properly. This is how a shell that knows the right answer supplies
    /// it, rather than each shell re-deriving the `http://host:port` shape from
    /// [`PairServer::port`] and drifting apart.
    ///
    /// `host` is checked against the same predicate as the peer check, so the one rule holds
    /// in both directions: **the server advertises only addresses it would answer on.** An
    /// address that fails it would produce a URL that the server itself refuses to serve —
    /// a QR code to nowhere, and a user staring at a page that will never load.
    ///
    /// # Errors
    /// [`PairError::NotOnPrivateNetwork`] if `host` is not private, link-local, or loopback;
    /// [`PairError::Bind`] if the socket cannot be opened.
    pub async fn start_advertising(
        host: Ipv4Addr,
        sink: Arc<dyn SubmissionSink>,
    ) -> Result<Self, PairError> {
        if !is_local(IpAddr::V4(host)) {
            return Err(PairError::NotOnPrivateNetwork {
                address: IpAddr::V4(host),
            });
        }
        let listener = TcpListener::bind((Ipv4Addr::UNSPECIFIED, 0))
            .await
            .map_err(PairError::Bind)?;
        let port = listener.local_addr().map_err(PairError::Bind)?.port();
        let url = format!("http://{host}:{port}");

        let token = Arc::new(PairToken::generate());
        let (shutdown, shutdown_rx) = watch::channel(false);
        let accept_task = tokio::spawn(serve(listener, Arc::clone(&token), sink, shutdown_rx));

        // The URL is not a secret — it is about to be printed on a television. The token is,
        // and is absent here by construction (§4.8).
        tracing::info!(target: TARGET, %url, "pairing server listening");
        Ok(Self {
            url,
            port,
            token,
            shutdown,
            accept_task: Some(accept_task),
        })
    }

    /// The URL to put on the TV: `http://192.168.x.y:PORT`.
    ///
    /// Always the address the server was started with, so it is only as good as that choice —
    /// see [`PairServer::start_advertising`]. Note that the URL is not load-bearing for
    /// security: a phone that finds the server by some other address is judged by the peer
    /// check exactly the same way.
    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }

    /// The bound port.
    #[must_use]
    pub fn port(&self) -> u16 {
        self.port
    }

    /// This session's token, for the TV screen. See [`PairToken::display`].
    #[must_use]
    pub fn token(&self) -> &PairToken {
        &self.token
    }

    /// Stops the server and waits for its task to finish.
    ///
    /// Dropping a [`PairServer`] stops it too; this is the same thing plus the wait, for a
    /// caller that needs the socket provably closed before it continues.
    pub async fn stop(mut self) {
        // A closed channel means the task already ended; either way the server is stopping.
        let _ = self.shutdown.send(true);
        if let Some(task) = self.accept_task.take() {
            let _ = task.await;
        }
    }
}

impl Drop for PairServer {
    fn drop(&mut self) {
        // "Exists only while its screen is visible" (§12), enforced here rather than by the
        // shell remembering to call `stop`. Signalling is enough — and even if this send
        // found no receiver, the sender's own drop ends the accept loop's `changed()`.
        let _ = self.shutdown.send(true);
    }
}

/// The accept loop. Ends on an explicit stop, or when [`PairServer`] drops and takes the
/// watch sender with it.
async fn serve(
    listener: TcpListener,
    token: Arc<PairToken>,
    sink: Arc<dyn SubmissionSink>,
    mut shutdown: watch::Receiver<bool>,
) {
    // A JoinSet, not bare spawns: in-flight handlers are owned by this task, so dropping it
    // at shutdown takes them with it and nothing outlives the screen.
    let mut connections = JoinSet::new();
    loop {
        // Backpressure at the cap. Every handler is bounded by READ_TIMEOUT, so waiting for
        // one to finish cannot wedge the loop.
        if connections.len() >= MAX_CONNECTIONS {
            tokio::select! {
                _ = shutdown.changed() => break,
                _ = connections.join_next() => {}
            }
            continue;
        }

        tokio::select! {
            _ = shutdown.changed() => break,
            accepted = listener.accept() => match accepted {
                Ok((stream, peer)) => {
                    connections.spawn(handle(stream, peer, Arc::clone(&token), Arc::clone(&sink)));
                }
                // Accept failures here are per-connection (a client that aborted between the
                // handshake and our accept), not listener death; the next iteration retries.
                Err(error) => tracing::debug!(target: TARGET, %error, "accept failed"),
            },
            // Reap finished handlers so the set cannot grow across a long-lived screen.
            Some(_) = connections.join_next(), if !connections.is_empty() => {}
        }
    }
    tracing::info!(target: TARGET, "pairing server stopped");
}

/// Serves one connection, start to finish.
async fn handle(
    mut stream: tokio::net::TcpStream,
    peer: SocketAddr,
    token: Arc<PairToken>,
    sink: Arc<dyn SubmissionSink>,
) {
    let (reader, mut writer) = stream.split();
    let mut reader = BufReader::new(reader);

    // The peer check runs before a single byte is read: an off-LAN caller never reaches the
    // parser, so the parser is not part of its attack surface at all.
    let outcome = if is_local(peer.ip()) {
        match timeout(READ_TIMEOUT, read_request(&mut reader)).await {
            Ok(Ok(request)) => route(&request, &token, &sink),
            Ok(Err(rejection)) => Err(rejection),
            Err(_elapsed) => Err(Rejection::Timeout),
        }
    } else {
        Err(Rejection::NotLocal)
    };

    let (status, body) = match outcome {
        Ok(page) => page,
        Err(rejection) => {
            log_rejection(rejection, peer);
            (rejection.status(), notice_for(rejection))
        }
    };
    write_response(&mut writer, status, &body).await;
}

/// Logs a refusal. The two that mean someone is probing are warnings; the rest are the noise
/// of the open internet and stay at debug.
///
/// Nothing here carries request bytes — no token candidate, no field values — so a rejection
/// is safe to log whole (TECH_SPEC §4.8).
fn log_rejection(rejection: Rejection, peer: SocketAddr) {
    let peer = peer.ip();
    match rejection {
        Rejection::NotLocal => tracing::warn!(
            target: TARGET, %peer,
            "refused a connection from outside the local network"
        ),
        Rejection::BadToken => tracing::warn!(
            target: TARGET, %peer,
            "refused a submission whose token did not match this session"
        ),
        other => tracing::debug!(target: TARGET, %peer, rejection = %other, "refused a request"),
    }
}

/// The page a refusal is served as. Copy is PRD §8.6: says what happened and what to do,
/// names no mechanism.
fn notice_for(rejection: Rejection) -> String {
    let (title, message) = match rejection {
        // Says only that it did not match — never whether a token was sent, nor how close it
        // was. There is one message here because there is one answer.
        Rejection::BadToken => (
            "That code doesn't match",
            "Check the code on your TV and type it again.",
        ),
        Rejection::NotLocal => (
            "Not available here",
            "Pairing only works from the same network as your TV.",
        ),
        Rejection::NoRoute => (
            "Nothing here",
            "The form is at the address shown on your TV.",
        ),
        Rejection::Timeout => ("That took too long", "Try again from your TV's address."),
        Rejection::RequestLineTooLong | Rejection::HeadersTooLarge | Rejection::BodyTooLarge => {
            ("That's too much to send", "Try again with a shorter link.")
        }
        Rejection::Malformed => (
            "That didn't come through",
            "Try again from the form on your TV.",
        ),
    };
    form::notice_page(title, message)
}

/// One parsed request. Only what the two routes read is kept.
struct Request {
    method: String,
    path: String,
    body: String,
}

/// Routes a request. Two routes exist; everything else is a 404.
fn route(
    request: &Request,
    token: &PairToken,
    sink: &Arc<dyn SubmissionSink>,
) -> Result<(Status, String), Rejection> {
    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/") => Ok((Status::OK, form::form_page(&Fields::default(), None))),
        ("POST", "/") => submit(&request.body, token, sink),
        _ => Err(Rejection::NoRoute),
    }
}

/// Handles `POST /`: the token first, then the form.
fn submit(
    body: &str,
    token: &PairToken,
    sink: &Arc<dyn SubmissionSink>,
) -> Result<(Status, String), Rejection> {
    let fields = form::parse_urlencoded(body)?;

    // Before anything else in this body is read as meaningful. A submission without the
    // token is a stranger's, and a stranger's fields are not worth validating.
    if !token.matches(fields.get(form::FIELD_TOKEN)) {
        return Err(Rejection::BadToken);
    }

    match form::submission_from(&fields) {
        Ok(submission) => {
            let page = form::confirmation_page(&submission);
            let kind = match &submission {
                Submission::M3uUrl { .. } => "m3u-url",
                Submission::Xtream { .. } => "xtream",
            };
            // The kind and nothing else: the URL may carry embedded auth and the password is
            // a `Secret` (§4.8, §12).
            tracing::info!(target: TARGET, kind, "accepted a pairing submission");
            sink.submit(submission);
            Ok((Status::OK, page))
        }
        // A bounced submission is not a rejection — it is the form again, with the values the
        // user already typed and a line saying what to fix.
        Err(invalid) => Ok((Status::BAD_REQUEST, form::form_page(&fields, Some(invalid)))),
    }
}

/// Reads and parses one HTTP/1.1 request, applying every cap as it goes.
async fn read_request<R: AsyncBufRead + Unpin>(reader: &mut R) -> Result<Request, Rejection> {
    let line = read_line(reader, MAX_REQUEST_LINE, Rejection::RequestLineTooLong).await?;
    let mut parts = line.split(' ');
    let (Some(method), Some(target), Some(version)) = (parts.next(), parts.next(), parts.next())
    else {
        return Err(Rejection::Malformed);
    };
    if !version.starts_with("HTTP/1.") {
        return Err(Rejection::Malformed);
    }
    // Neither route reads a query or fragment, so they are dropped here rather than left for
    // routing to trip over: `/?x=1` is the form, not a 404.
    let path = target.split(['?', '#']).next().unwrap_or("/").to_owned();
    let method = method.to_owned();

    let mut content_length = 0_usize;
    let mut header_bytes = 0_usize;
    let mut header_count = 0_usize;
    loop {
        let line = read_line(reader, MAX_HEADER_BYTES, Rejection::HeadersTooLarge).await?;
        if line.is_empty() {
            break; // the blank line that ends the header block
        }
        header_count += 1;
        header_bytes += line.len();
        if header_count > MAX_HEADERS || header_bytes > MAX_HEADER_BYTES {
            return Err(Rejection::HeadersTooLarge);
        }
        let Some((name, value)) = line.split_once(':') else {
            return Err(Rejection::Malformed);
        };
        if name.eq_ignore_ascii_case("transfer-encoding") {
            // The only body that exists here is a browser form POST, which always sends
            // Content-Length. Refusing chunked outright leaves exactly one way to frame a
            // body, so there is no second framing to disagree with the first.
            return Err(Rejection::Malformed);
        }
        if name.eq_ignore_ascii_case("content-length") {
            content_length = value.trim().parse().map_err(|_| Rejection::Malformed)?;
            if content_length > MAX_BODY {
                return Err(Rejection::BodyTooLarge);
            }
        }
    }

    // Bounded by construction: `content_length` was capped above, so this allocates at most
    // MAX_BODY no matter what the client claimed, and `read_exact` cannot be told to read
    // more than was allocated.
    let mut body = vec![0_u8; content_length];
    reader
        .read_exact(&mut body)
        .await
        .map_err(|_| Rejection::Malformed)?;
    let body = String::from_utf8(body).map_err(|_| Rejection::Malformed)?;

    Ok(Request { method, path, body })
}

/// Reads one CRLF-terminated line, refusing to buffer more than `limit` bytes.
///
/// `take` is what makes the cap real: it bounds what can be pulled from the socket, so an
/// endless line without a newline is refused after `limit` bytes rather than filling memory.
/// `overflow` is the rejection to raise when the cap arrives before the newline does — the
/// caller names it, because the same read is a 414 on the request line and a 431 in headers.
async fn read_line<R: AsyncBufRead + Unpin>(
    reader: &mut R,
    limit: usize,
    overflow: Rejection,
) -> Result<String, Rejection> {
    let mut buf = Vec::new();
    let read = (&mut *reader)
        .take(limit as u64)
        .read_until(b'\n', &mut buf)
        .await
        .map_err(|_| Rejection::Malformed)?;
    if read == 0 {
        return Err(Rejection::Malformed); // the client went away mid-request
    }
    if !buf.ends_with(b"\n") {
        return Err(overflow);
    }
    let line = String::from_utf8(buf).map_err(|_| Rejection::Malformed)?;
    Ok(line.trim_end_matches(['\r', '\n']).to_owned())
}

/// Writes one response and closes.
///
/// Write failures are dropped: they mean the client hung up, and there is no one left to
/// tell. The headers are the whole security posture of the served page — a CSP that permits
/// the inline stylesheet and nothing else (no script, no subresource, no other form target),
/// `no-store` because the page carries a code the user typed, and `nosniff` so the browser
/// cannot be talked out of treating it as HTML.
async fn write_response<W: AsyncWrite + Unpin>(writer: &mut W, status: Status, body: &str) {
    let head = format!(
        "HTTP/1.1 {code} {reason}\r\n\
         Content-Type: text/html; charset=utf-8\r\n\
         Content-Length: {len}\r\n\
         Cache-Control: no-store\r\n\
         Content-Security-Policy: default-src 'none'; style-src 'unsafe-inline'; \
         form-action 'self'; base-uri 'none'\r\n\
         Referrer-Policy: no-referrer\r\n\
         X-Content-Type-Options: nosniff\r\n\
         Connection: close\r\n\
         \r\n",
        code = status.code,
        reason = status.reason,
        len = body.len(),
    );
    let _ = timeout(WRITE_TIMEOUT, async {
        writer.write_all(head.as_bytes()).await?;
        writer.write_all(body.as_bytes()).await?;
        writer.flush().await
    })
    .await;
}

/// Whether a peer is somewhere a TV's phone could plausibly be.
///
/// The teeth behind §12's "a person on the network cannot inject a source into a TV they
/// cannot see": the bind is `0.0.0.0`, so this is what actually decides who is answered.
/// Private (RFC 1918), link-local (169.254/16, for a TV and phone on an ad-hoc link), and
/// loopback (the shells' own contract tests).
fn is_local(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_private() || v4.is_link_local() || v4.is_loopback(),
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            // A dual-stack socket reports an IPv4 peer as ::ffff:a.b.c.d — judge the real
            // address, not its wrapper.
            Some(v4) => is_local(IpAddr::V4(v4)),
            // `Ipv6Addr::is_unique_local` and `is_unicast_link_local` are still unstable, so
            // the prefixes are matched by hand: fc00::/7 unique-local, fe80::/10 link-local.
            None => {
                v6.is_loopback()
                    || (v6.segments()[0] & 0xfe00) == 0xfc00
                    || (v6.segments()[0] & 0xffc0) == 0xfe80
            }
        },
    }
}

/// Infers the host's private IPv4 — the address a phone has to dial.
///
/// Found by asking the kernel which interface it would use to reach the outside, without
/// sending anything: `connect` on a UDP socket transmits no packet, it only resolves a route
/// and binds the socket to that route's source address. Reading `local_addr` afterwards
/// yields the IP of the interface that reaches the default gateway, which on a plain LAN is
/// the address the phone must dial. The destination is TEST-NET-1 (RFC 5737), reserved for
/// documentation and routed nowhere; it exists only to name a direction.
///
/// **This is an inference, and its limit is load-bearing.** It answers "where do I leave
/// from", which is only the same question as "where can the phone find me" when one interface
/// does both. A full-tunnel VPN breaks that: the route out belongs to the tunnel, so this
/// returns the tunnel's address while the LAN address sits unused on another interface.
/// Measured on a host with Wi-Fi at `192.168.50.98` and a VPN at `100.80.166.175`, every
/// probe destination — public, private, broadcast, multicast — returned the tunnel. Only a
/// destination inside the real subnet returned the Wi-Fi address, and that prefix is the very
/// thing we are trying to learn, so the trick cannot be rescued by choosing a better
/// destination. Answering properly means enumerating interfaces, which needs a dependency
/// this crate does not have (crate docs). [`PairServer::start_advertising`] is the way out.
///
/// The validation below is strict on purpose and must stay that way. It is not the peer
/// check — an address failing here is refused rather than advertised — but relaxing it (to
/// admit RFC 6598 CGNAT space, say) would put an address on a TV that a stranger sharing that
/// carrier pool might also reach, which is the exposure the peer check exists to prevent.
///
/// # Errors
/// [`PairError::NoLanAddress`] if there is no route to infer from at all;
/// [`PairError::NotOnPrivateNetwork`] if the route's source address is not dialable from a
/// LAN — which, per that variant's docs, does *not* prove the TV is off the network.
async fn private_ipv4() -> Result<Ipv4Addr, PairError> {
    let local = route_source_address()
        .await
        .map_err(PairError::NoLanAddress)?;
    let IpAddr::V4(host) = local else {
        // Not reachable in practice — the probe binds an IPv4 wildcard — but a v6 source
        // address is not something to advertise on a pairing screen either.
        return Err(PairError::NotOnPrivateNetwork { address: local });
    };
    if !(host.is_private() || host.is_link_local() || host.is_loopback()) {
        tracing::warn!(
            target: TARGET, %host,
            "the route out of this host does not start from a private address (a full-tunnel \
             VPN does this on a perfectly connected TV); refusing to advertise an address a \
             phone on the local network cannot reach"
        );
        return Err(PairError::NotOnPrivateNetwork {
            address: IpAddr::V4(host),
        });
    }
    Ok(host)
}

/// The source address the kernel would use to reach the outside world.
async fn route_source_address() -> std::io::Result<IpAddr> {
    let probe = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).await?;
    probe.connect((Ipv4Addr::new(192, 0, 2, 1), 9)).await?;
    Ok(probe.local_addr()?.ip())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn every_private_range_is_local() {
        for ip in [
            "10.0.0.1",           // RFC 1918 10/8
            "10.255.255.255",     //
            "172.16.0.1",         // RFC 1918 172.16/12
            "172.31.255.254",     //
            "192.168.1.42",       // RFC 1918 192.168/16
            "169.254.10.1",       // link-local
            "127.0.0.1",          // loopback
            "::1",                // IPv6 loopback
            "fd00::1",            // IPv6 unique-local
            "fe80::1",            // IPv6 link-local
            "::ffff:192.168.0.5", // IPv4-mapped private
        ] {
            let parsed: IpAddr = ip.parse().unwrap();
            assert!(is_local(parsed), "{ip} should be treated as local");
        }
    }

    #[test]
    fn the_public_internet_is_not_local() {
        for ip in [
            "8.8.8.8",
            "1.1.1.1",
            "203.0.113.7",    // TEST-NET-3, but still a public range
            "172.32.0.1",     // just past 172.16/12 — the classic off-by-one
            "172.15.255.255", // just before it
            "192.169.0.1",    // just past 192.168/16
            "11.0.0.1",       // just past 10/8
            "2606:4700::1",   // public IPv6
            "::ffff:8.8.8.8", // IPv4-mapped public
        ] {
            let parsed: IpAddr = ip.parse().unwrap();
            assert!(!is_local(parsed), "{ip} must not be treated as local");
        }
    }

    #[test]
    fn every_rejection_is_served_as_a_page_that_offers_the_source() {
        // `notice_for` is the only path a refusal takes to the wire, so covering it here
        // covers every 4xx the server can emit.
        let all = [
            Rejection::NotLocal,
            Rejection::BadToken,
            Rejection::Timeout,
            Rejection::RequestLineTooLong,
            Rejection::HeadersTooLarge,
            Rejection::BodyTooLarge,
            Rejection::Malformed,
            Rejection::NoRoute,
        ];
        for rejection in all {
            let page = notice_for(rejection);
            assert!(
                page.contains(env!("CARGO_PKG_REPOSITORY")),
                "{rejection:?} served a page without the source link"
            );
        }
    }

    #[test]
    fn a_refused_token_page_says_nothing_about_the_token() {
        let page = notice_for(Rejection::BadToken);
        for tell in ["missing", "expected", "characters", "length", "close"] {
            assert!(
                !page.to_lowercase().contains(tell),
                "the page hints at why the token failed with {tell:?}: {page}"
            );
        }
    }
}
