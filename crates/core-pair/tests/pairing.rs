// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! End-to-end tests against a really-bound pairing server, spoken to over real TCP.
//!
//! The unit tests prove the pieces; these prove the thing a phone actually meets. The client
//! is written by hand for the same reason the server is (`core-pair`'s manifest): a test that
//! goes through an HTTP library tests the library's idea of the request, not the bytes a
//! hostile client would send. Here the bytes are the test — which is what lets the hardening
//! cases below send requests no library would agree to construct.
//!
//! **Environment:** these are hermetic. They start the server with
//! [`PairServer::start_advertising`] at [`Ipv4Addr::LOCALHOST`] and drive it over loopback,
//! so nothing here depends on the host's networking. That matters: `PairServer::start` infers
//! an address from the route out, which fails on a dev box behind a VPN and on some CI
//! runners — testing the request/response surface through it would make this suite red for
//! reasons that have nothing to do with the server. The inference is a separate concern and
//! is tested separately (`is_local`'s range predicate, in unit tests).
//!
//! An off-LAN peer cannot be staged from loopback, so the locality rule itself is likewise
//! covered by the `is_local` unit tests rather than here.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fmt::Write as _;
use std::net::Ipv4Addr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use core_pair::{PairError, PairServer, Submission, SubmissionSink};

/// The repository the AGPL §13 colophon must offer on every page.
const SOURCE_URL: &str = "https://github.com/edbpede/spidola";

/// A sink that keeps what it is given, so tests can ask what reached the shell.
#[derive(Clone, Default)]
struct Recorder {
    received: Arc<Mutex<Vec<Submission>>>,
}

impl SubmissionSink for Recorder {
    fn submit(&self, submission: Submission) {
        self.received.lock().unwrap().push(submission);
    }
}

impl Recorder {
    fn count(&self) -> usize {
        self.received.lock().unwrap().len()
    }
}

/// One parsed HTTP response.
struct Response {
    status: u16,
    head: String,
    body: String,
}

impl Response {
    fn parse(raw: &str) -> Self {
        let (head, body) = raw
            .split_once("\r\n\r\n")
            .expect("a response must have a header block");
        let status = head
            .lines()
            .next()
            .and_then(|line| line.split(' ').nth(1))
            .and_then(|code| code.parse().ok())
            .expect("a response must have a status line");
        Self {
            status,
            head: head.to_owned(),
            body: body.to_owned(),
        }
    }
}

/// Starts a server with a recording sink, advertising loopback (see the module docs).
async fn start() -> (PairServer, Recorder) {
    let recorder = Recorder::default();
    let server = PairServer::start_advertising(Ipv4Addr::LOCALHOST, Arc::new(recorder.clone()))
        .await
        .expect("loopback is dialable and an ephemeral port always binds");
    (server, recorder)
}

/// Sends raw bytes to the server and reads the whole response. The server always answers
/// `Connection: close`, so reading to EOF is reading exactly one response.
async fn send(port: u16, raw: &str) -> Response {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    stream.write_all(raw.as_bytes()).await.unwrap();
    let mut buf = String::new();
    stream.read_to_string(&mut buf).await.unwrap();
    Response::parse(&buf)
}

fn get(path: &str) -> String {
    format!("GET {path} HTTP/1.1\r\nHost: tv.local\r\n\r\n")
}

fn post(body: &str) -> String {
    format!(
        "POST / HTTP/1.1\r\nHost: tv.local\r\n\
         Content-Type: application/x-www-form-urlencoded\r\n\
         Content-Length: {}\r\n\r\n{body}",
        body.len()
    )
}

// ---------------------------------------------------------------------------
// AGPL §13 — the compliance proof (PRD §10, TECH_SPEC §12)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn every_served_response_offers_the_source() {
    // The compliance claim in one test: not "the form has a link" but "there is no way to
    // get bytes out of this server that do not carry the offer". So it asks for every kind
    // of response the server can produce — the form, a good submission, a bounced one, a
    // refused token, a 404, and each hardening refusal — and checks all of them.
    let (server, _recorder) = start().await;
    let port = server.port();
    let token = server.token().display().to_owned();

    let responses = vec![
        send(port, &get("/")).await,
        send(port, &get("/nope")).await,
        send(
            port,
            &post(&format!("token={token}&kind=m3u-url&url=not+a+url")),
        )
        .await,
        send(
            port,
            &post("token=WRONG1&kind=m3u-url&url=http%3A%2F%2Fa.example"),
        )
        .await,
        send(port, &post("kind=m3u-url&url=http%3A%2F%2Fa.example")).await,
        send(
            port,
            &format!("GET /{} HTTP/1.1\r\nHost: t\r\n\r\n", "a".repeat(3000)),
        )
        .await,
        send(
            port,
            &format!(
                "GET / HTTP/1.1\r\nHost: t\r\nX-Long: {}\r\n\r\n",
                "a".repeat(9000)
            ),
        )
        .await,
        send(
            port,
            "POST / HTTP/1.1\r\nHost: t\r\nContent-Length: 99999\r\n\r\n",
        )
        .await,
        send(port, "GARBAGE\r\n\r\n").await,
        // Last, because it is the one that consumes the token.
        send(
            port,
            &post(&format!(
                "token={token}&kind=m3u-url&url=http%3A%2F%2Fa.example%2Fl.m3u"
            )),
        )
        .await,
    ];

    for response in &responses {
        assert!(
            response.body.contains(SOURCE_URL),
            "a {} response omits the Corresponding Source link:\n{}",
            response.status,
            response.body
        );
        assert!(
            response.body.contains("Source code</a>"),
            "the offer must be a real anchor, not bare text:\n{}",
            response.body
        );
    }
}

// ---------------------------------------------------------------------------
// The two routes
// ---------------------------------------------------------------------------

#[tokio::test]
async fn get_serves_the_form() {
    let (server, _recorder) = start().await;
    let response = send(server.port(), &get("/")).await;

    assert_eq!(response.status, 200);
    assert!(
        response
            .head
            .contains("Content-Type: text/html; charset=utf-8")
    );
    assert!(response.body.contains("Add a source"));
    assert!(response.body.contains("name=\"token\""));
    assert!(response.body.contains("value=\"m3u-url\""));
    assert!(response.body.contains("value=\"xtream\""));
}

#[tokio::test]
async fn the_form_never_carries_the_token() {
    // The token's whole security value is that it travels by line of sight. Serving it would
    // hand it to exactly the person it exists to exclude (TECH_SPEC §12).
    let (server, _recorder) = start().await;
    let token = server.token().display().to_owned();
    let response = send(server.port(), &get("/")).await;

    assert!(
        !response.body.contains(&token),
        "the served form leaked this session's token"
    );
    assert!(!response.head.contains(&token), "a header leaked the token");
}

#[tokio::test]
async fn a_query_string_still_reaches_the_form() {
    let (server, _recorder) = start().await;
    let response = send(server.port(), &get("/?utm=whatever")).await;
    assert_eq!(response.status, 200);
    assert!(response.body.contains("Add a source"));
}

#[tokio::test]
async fn an_unknown_path_is_a_404() {
    let (server, _recorder) = start().await;
    for path in ["/nope", "/favicon.ico", "/../etc/passwd", "/admin"] {
        let response = send(server.port(), &get(path)).await;
        assert_eq!(response.status, 404, "{path} should not exist");
    }
}

#[tokio::test]
async fn an_unsupported_method_is_a_404() {
    let (server, _recorder) = start().await;
    for method in ["PUT", "DELETE", "HEAD", "OPTIONS", "TRACE"] {
        let response = send(
            server.port(),
            &format!("{method} / HTTP/1.1\r\nHost: t\r\n\r\n"),
        )
        .await;
        assert_eq!(response.status, 404, "{method} / should not exist");
    }
}

// ---------------------------------------------------------------------------
// Submissions
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_playlist_submission_reaches_the_sink() {
    let (server, recorder) = start().await;
    let token = server.token().display().to_owned();

    let response = send(
        server.port(),
        &post(&format!(
            "token={token}&kind=m3u-url&url=http%3A%2F%2Fa.example%2Flist.m3u"
        )),
    )
    .await;

    assert_eq!(response.status, 200);
    assert!(response.body.contains("That's on your TV now"));

    let received = recorder.received.lock().unwrap();
    assert_eq!(received.len(), 1);
    let Submission::M3uUrl { url } = &received[0] else {
        panic!("expected a playlist submission, got {:?}", received[0]);
    };
    assert_eq!(url.as_str(), "http://a.example/list.m3u");
}

#[tokio::test]
async fn an_xtream_submission_reaches_the_sink_with_its_password() {
    let (server, recorder) = start().await;
    let token = server.token().display().to_owned();

    let response = send(
        server.port(),
        &post(&format!(
            "token={token}&kind=xtream&server=http%3A%2F%2Fpanel.example%3A8080\
             &username=alice&password=hunter2%21%40%23"
        )),
    )
    .await;

    assert_eq!(response.status, 200);

    let received = recorder.received.lock().unwrap();
    assert_eq!(received.len(), 1);
    let Submission::Xtream {
        server,
        username,
        password,
    } = &received[0]
    else {
        panic!("expected an Xtream submission, got {:?}", received[0]);
    };
    assert_eq!(server.as_str(), "http://panel.example:8080");
    assert_eq!(username, "alice");
    // The password survives percent-decoding intact, punctuation and all.
    assert_eq!(password.expose(), "hunter2!@#");
}

#[tokio::test]
async fn the_lowercase_token_a_phone_keyboard_sends_is_accepted() {
    let (server, recorder) = start().await;
    let token = server.token().display().to_ascii_lowercase();

    let response = send(
        server.port(),
        &post(&format!(
            "token={token}&kind=m3u-url&url=http%3A%2F%2Fa.example%2Fl.m3u"
        )),
    )
    .await;

    assert_eq!(response.status, 200);
    assert_eq!(recorder.count(), 1);
}

#[tokio::test]
async fn a_submitted_password_never_comes_back_out() {
    let (server, _recorder) = start().await;
    let token = server.token().display().to_owned();

    // Bounce it on the server URL, so the form is re-rendered with the password in hand.
    let response = send(
        server.port(),
        &post(&format!(
            "token={token}&kind=xtream&server=nonsense&username=alice\
             &password=hunter2-top-secret"
        )),
    )
    .await;

    assert_eq!(response.status, 400);
    assert!(
        !response.body.contains("hunter2"),
        "the bounced form echoed the password back:\n{}",
        response.body
    );
    assert!(
        !response.head.contains("hunter2"),
        "a response header carried the password"
    );
    // The values that are not credentials do come back, so the user need not retype them.
    assert!(response.body.contains("value=\"alice\""));
}

#[tokio::test]
async fn a_bounced_submission_returns_the_form_and_not_the_sink() {
    let (server, recorder) = start().await;
    let token = server.token().display().to_owned();

    let response = send(
        server.port(),
        &post(&format!("token={token}&kind=m3u-url&url=not+a+url")),
    )
    .await;

    assert_eq!(response.status, 400);
    assert!(response.body.contains("That doesn't look like a link"));
    assert!(
        response.body.contains("Add a source"),
        "the form comes back"
    );
    assert_eq!(
        recorder.count(),
        0,
        "an invalid submission must not reach the shell"
    );
}

// ---------------------------------------------------------------------------
// The token gate
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_wrong_token_is_refused_and_reaches_nothing() {
    let (server, recorder) = start().await;
    let response = send(
        server.port(),
        &post("token=ZZZZZZ&kind=m3u-url&url=http%3A%2F%2Fevil.example%2Fl.m3u"),
    )
    .await;

    assert_eq!(response.status, 403);
    assert_eq!(
        recorder.count(),
        0,
        "an unauthenticated source was injected"
    );
}

#[tokio::test]
async fn a_missing_token_is_refused_identically_to_a_wrong_one() {
    // The whole point: a guesser must not be able to tell "you sent no token" from "you sent
    // the wrong token", nor learn anything about how close they were. Same status, same
    // bytes, and — since the token is fixed-length and compared without short-circuiting —
    // no signal left to read.
    let (server, _recorder) = start().await;
    let port = server.port();
    let body = "kind=m3u-url&url=http%3A%2F%2Fa.example";

    let missing = send(port, &post(body)).await;
    let wrong = send(port, &post(&format!("token=ZZZZZZ&{body}"))).await;
    let nearly = {
        // Right length, right alphabet, one character off.
        let token = server.token().display().to_owned();
        let mut near = token.clone().into_bytes();
        near[5] = if near[5] == b'2' { b'3' } else { b'2' };
        let near = String::from_utf8(near).unwrap();
        assert_ne!(near, token, "the near-miss must not be the real token");
        send(port, &post(&format!("token={near}&{body}"))).await
    };

    assert_eq!(missing.status, 403);
    assert_eq!(wrong.status, 403);
    assert_eq!(nearly.status, 403);
    assert_eq!(
        missing.body, wrong.body,
        "a missing token is distinguishable"
    );
    assert_eq!(wrong.body, nearly.body, "a near miss is distinguishable");
}

#[tokio::test]
async fn one_servers_token_does_not_open_another() {
    let (first, first_sink) = start().await;
    let (second, second_sink) = start().await;

    let response = send(
        second.port(),
        &post(&format!(
            "token={}&kind=m3u-url&url=http%3A%2F%2Fa.example%2Fl.m3u",
            first.token().display()
        )),
    )
    .await;

    assert_eq!(response.status, 403);
    assert_eq!(first_sink.count(), 0);
    assert_eq!(second_sink.count(), 0);
}

// ---------------------------------------------------------------------------
// Lifecycle — "alive only while its screen is visible" (TECH_SPEC §12)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn stop_closes_the_socket_before_it_returns() {
    let (server, _recorder) = start().await;
    let port = server.port();
    assert_eq!(send(port, &get("/")).await.status, 200, "serving first");

    server.stop().await;

    // No polling: `stop` awaits the accept task, which drops the listener on its way out, so
    // by the time it returns the port is provably gone.
    assert!(
        TcpStream::connect(("127.0.0.1", port)).await.is_err(),
        "the socket is still open after stop() returned"
    );
}

#[tokio::test]
async fn dropping_the_handle_stops_the_server() {
    // The rule that matters, because it is the one a shell can forget: the server dies when
    // its screen does, whether or not anyone said stop.
    let port = {
        let (server, _recorder) = start().await;
        let port = server.port();
        assert_eq!(send(port, &get("/")).await.status, 200, "serving first");
        port
    };

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if TcpStream::connect(("127.0.0.1", port)).await.is_err() {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "port {port} still accepts connections after the handle dropped"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test]
async fn each_session_gets_its_own_token_and_port() {
    let (first, _a) = start().await;
    let (second, _b) = start().await;

    assert_ne!(first.port(), second.port());
    assert_ne!(first.token().display(), second.token().display());
}

#[tokio::test]
async fn the_advertised_url_names_this_server_at_the_requested_address() {
    let (server, _recorder) = start().await;
    assert_eq!(server.url(), format!("http://127.0.0.1:{}", server.port()));
    // And the server really is there, at the URL it advertises.
    assert_eq!(send(server.port(), &get("/")).await.status, 200);
}

#[tokio::test]
async fn an_address_the_server_would_not_answer_on_cannot_be_advertised() {
    // The invariant behind `start_advertising`: advertise only what the peer check would
    // accept. A public address here would put a URL on a TV that the server itself refuses to
    // serve — so it is refused up front rather than becoming a QR code to nowhere.
    for host in [
        Ipv4Addr::new(8, 8, 8, 8),
        Ipv4Addr::new(100, 80, 166, 175), // RFC 6598 CGNAT — what a VPN'd host infers
        Ipv4Addr::new(172, 32, 0, 1),     // just past RFC 1918's 172.16/12
    ] {
        let started = PairServer::start_advertising(host, Arc::new(Recorder::default())).await;
        let Err(PairError::NotOnPrivateNetwork { address }) = started else {
            panic!("{host} should not be advertisable");
        };
        assert_eq!(address, std::net::IpAddr::V4(host));
    }
}

#[tokio::test]
async fn every_address_the_server_answers_on_can_be_advertised() {
    for host in [
        Ipv4Addr::LOCALHOST,
        Ipv4Addr::new(192, 168, 1, 42),
        Ipv4Addr::new(10, 0, 0, 1),
        Ipv4Addr::new(172, 16, 0, 1),
        Ipv4Addr::new(169, 254, 10, 1),
    ] {
        let server = PairServer::start_advertising(host, Arc::new(Recorder::default()))
            .await
            .unwrap_or_else(|e| panic!("{host} should be advertisable: {e}"));
        assert_eq!(server.url(), format!("http://{host}:{}", server.port()));
    }
}

#[tokio::test]
async fn the_handle_cannot_debug_print_its_token() {
    // `PairServer` derives Debug, so this asserts the redaction actually holds through it —
    // the realistic leak is a shell logging the whole handle, not the token itself.
    let (server, _recorder) = start().await;
    let rendered = format!("{server:?}");
    assert!(
        !rendered.contains(server.token().display()),
        "the server's Debug output leaked the token: {rendered}"
    );
    assert!(rendered.contains("REDACTED"));
}

// ---------------------------------------------------------------------------
// Hardening — the budget Phase 7 will shoot at
// ---------------------------------------------------------------------------

#[tokio::test]
async fn an_over_long_request_line_is_refused() {
    let (server, _recorder) = start().await;
    let response = send(
        server.port(),
        &format!("GET /{} HTTP/1.1\r\nHost: t\r\n\r\n", "a".repeat(4000)),
    )
    .await;
    assert_eq!(response.status, 414);
}

#[tokio::test]
async fn an_over_long_header_is_refused() {
    let (server, _recorder) = start().await;
    let response = send(
        server.port(),
        &format!(
            "GET / HTTP/1.1\r\nHost: t\r\nX-Fat: {}\r\n\r\n",
            "a".repeat(9000)
        ),
    )
    .await;
    assert_eq!(response.status, 431);
}

#[tokio::test]
async fn too_many_headers_are_refused() {
    let (server, _recorder) = start().await;
    let headers = (0..64).fold(String::new(), |mut acc, i| {
        let _ = writeln!(acc, "X-Pad-{i}: v\r");
        acc
    });
    let response = send(
        server.port(),
        &format!("GET / HTTP/1.1\r\nHost: t\r\n{headers}\r\n"),
    )
    .await;
    assert_eq!(response.status, 431);
}

#[tokio::test]
async fn an_over_large_body_is_refused_on_its_claim_alone() {
    // The claim is refused before a byte of the body is read, so a lying Content-Length costs
    // the server nothing — it never allocates what it was told to expect.
    let (server, _recorder) = start().await;
    let response = send(
        server.port(),
        "POST / HTTP/1.1\r\nHost: t\r\nContent-Length: 10485760\r\n\r\n",
    )
    .await;
    assert_eq!(response.status, 413);
}

#[tokio::test]
async fn a_chunked_body_is_refused() {
    // Exactly one way to frame a body exists here, so there is no second framing to disagree
    // with the first.
    let (server, _recorder) = start().await;
    let response = send(
        server.port(),
        "POST / HTTP/1.1\r\nHost: t\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n",
    )
    .await;
    assert_eq!(response.status, 400);
}

#[tokio::test]
async fn a_garbage_request_is_refused() {
    let (server, _recorder) = start().await;
    for raw in [
        "GARBAGE\r\n\r\n",
        "GET\r\n\r\n",
        "GET / SPDY/9.9\r\n\r\n",
        "GET / HTTP/1.1\r\nnot-a-header-line\r\n\r\n",
        "\r\n\r\n",
    ] {
        let response = send(server.port(), raw).await;
        assert_eq!(response.status, 400, "should have refused: {raw:?}");
    }
}

#[tokio::test]
async fn a_client_that_dribbles_is_cut_off() {
    // Slow-loris: a request that never ends. The read budget is what stops one connection
    // from holding a task forever.
    let (server, _recorder) = start().await;
    let mut stream = TcpStream::connect(("127.0.0.1", server.port()))
        .await
        .unwrap();
    // A request line and a header, but never the blank line that would finish the block.
    stream
        .write_all(b"GET / HTTP/1.1\r\nHost: tv.local\r\n")
        .await
        .unwrap();

    let started = Instant::now();
    let mut buf = String::new();
    stream.read_to_string(&mut buf).await.unwrap();
    let elapsed = started.elapsed();

    assert_eq!(Response::parse(&buf).status, 408);
    assert!(
        elapsed < Duration::from_secs(30),
        "the connection was held for {elapsed:?} — the read budget did not fire"
    );
}

#[tokio::test]
async fn the_response_headers_lock_the_page_down() {
    let (server, _recorder) = start().await;
    let response = send(server.port(), &get("/")).await;

    // No script, no subresource, no other form target — asserted on the wire, because the CSP
    // is what makes "self-contained" enforceable rather than merely intended.
    assert!(
        response
            .head
            .contains("Content-Security-Policy: default-src 'none'")
    );
    assert!(response.head.contains("form-action 'self'"));
    assert!(response.head.contains("X-Content-Type-Options: nosniff"));
    assert!(response.head.contains("Referrer-Policy: no-referrer"));
    // The page carries a code the user typed; it has no business in a phone's cache.
    assert!(response.head.contains("Cache-Control: no-store"));
}

#[tokio::test]
async fn many_connections_at_once_do_not_break_the_server() {
    // More than the concurrency cap, so the backpressure path is exercised rather than
    // assumed. Every one of them must still be answered.
    let (server, _recorder) = start().await;
    let port = server.port();

    let mut handles = Vec::new();
    for _ in 0..48 {
        handles.push(tokio::spawn(async move { send(port, &get("/")).await }));
    }
    for handle in handles {
        assert_eq!(handle.await.unwrap().status, 200);
    }
    // And the server is still there afterwards.
    assert_eq!(send(port, &get("/")).await.status, 200);
}
