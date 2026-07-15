// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! End-to-end Xtream tests against a dependency-free local HTTP stub, driving the real
//! fixture corpus through the real `core-fetch` client.
//!
//! The unit tests in `src/` pin each mapping decision against a JSON literal; these pin the
//! wiring the unit tests cannot see — that the audited URL `crate::urls` builds is the one
//! that actually reaches the wire, credentials and action intact, and that a headend's
//! failures land in the right arm of the taxonomy.
//!
//! The stub follows `core-fetch`'s own `tests/streaming.rs` pattern (a raw `TcpListener`,
//! no HTTP-server dependency), extended to capture the request line so a test can assert
//! what the client actually sent.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{self, Receiver};
use std::thread;

use core_fetch::{FetchConfig, HttpClient};
use core_model::channel::MediaKind;
use core_model::locator::StreamLocator;
use core_model::secret::Secret;
use core_xtream::diagnostics::SkipReason;
use core_xtream::error::{AuthRejection, XtreamError};
use core_xtream::urls::Endpoint;
use core_xtream::{auth, catalog, series};

/// Loads a fixture from the repository corpus.
fn fixture(name: &str) -> Vec<u8> {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/xtream/");
    std::fs::read(format!("{path}{name}"))
        .unwrap_or_else(|e| panic!("failed to read fixture {name}: {e}"))
}

/// A stub headend. Serves one canned response per connection and records what was asked.
struct Stub {
    base: String,
    requests: Receiver<String>,
}

/// Spawns a stub that answers `count` requests with `status` and `body`, recording the
/// request target of each.
///
/// Serves sequentially on one thread: every call in these tests is a single request, and a
/// serial stub keeps the recorded order meaningful.
fn spawn_stub(status: &'static str, body: Vec<u8>, count: usize) -> Stub {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let (tx, requests) = mpsc::channel();
    thread::spawn(move || {
        for _ in 0..count {
            let Ok((mut stream, _)) = listener.accept() else {
                break;
            };
            let target = read_request_target(&mut stream);
            let _ = tx.send(target);
            let header = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\n\
                 Content-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(header.as_bytes());
            let _ = stream.write_all(&body);
            let _ = stream.flush();
        }
    });
    Stub {
        base: format!("http://{addr}"),
        requests,
    }
}

/// Reads the request headers and returns the request target (`GET <target> HTTP/1.1`).
fn read_request_target(stream: &mut TcpStream) -> String {
    let mut buf = [0_u8; 4096];
    let mut seen = Vec::new();
    while let Ok(n) = stream.read(&mut buf) {
        if n == 0 {
            break;
        }
        seen.extend_from_slice(&buf[..n]);
        if seen.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }
    let text = String::from_utf8_lossy(&seen);
    text.lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or_default()
        .to_owned()
}

impl Stub {
    fn endpoint(&self, username: &str) -> Endpoint {
        Endpoint::new(&StreamLocator::parse(&self.base).unwrap(), username).unwrap()
    }

    /// The target of the next request the stub served.
    fn next_request(&self) -> String {
        self.requests
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("the stub never received a request")
    }
}

fn client() -> HttpClient {
    HttpClient::new(&FetchConfig::default()).unwrap()
}

// ---- The wire-up: what actually reaches the headend ---------------------------------

#[tokio::test]
async fn the_request_carries_the_audited_credentials_and_action() {
    let stub = spawn_stub("200 OK", fixture("live-categories.json"), 1);
    let endpoint = stub.endpoint("alice");

    catalog::categories(
        &client(),
        &endpoint,
        &Secret::new("hunter2"),
        MediaKind::Live,
    )
    .await
    .unwrap();

    // The URL `urls::player_api` built is the one that left the process — proving the
    // audited construction is on the live path, not merely unit-tested beside it.
    assert_eq!(
        stub.next_request(),
        "/player_api.php?username=alice&password=hunter2&action=get_live_categories"
    );
}

#[tokio::test]
async fn a_password_full_of_url_metacharacters_survives_the_round_trip() {
    let stub = spawn_stub("200 OK", fixture("handshake-active.json"), 1);
    let endpoint = stub.endpoint("user name");

    auth::authenticate(&client(), &endpoint, &Secret::new("p/a?s&s=x"))
        .await
        .unwrap();

    // Every metacharacter is escaped, so the query still has exactly two parameters before
    // the action — the password cannot forge one.
    assert_eq!(
        stub.next_request(),
        "/player_api.php?username=user+name&password=p%2Fa%3Fs%26s%3Dx"
    );
}

#[tokio::test]
async fn a_category_filter_reaches_the_headend() {
    let stub = spawn_stub("200 OK", fixture("live-streams.json"), 1);
    let endpoint = stub.endpoint("alice");

    catalog::live_streams(&client(), &endpoint, &Secret::new("pw"), &[], Some("2"))
        .await
        .unwrap();

    assert_eq!(
        stub.next_request(),
        "/player_api.php?username=alice&password=pw&action=get_live_streams&category_id=2"
    );
}

// ---- The handshake ------------------------------------------------------------------

#[tokio::test]
async fn an_active_account_authenticates_from_the_fixture() {
    let stub = spawn_stub("200 OK", fixture("handshake-active.json"), 1);
    let status = auth::authenticate(&client(), &stub.endpoint("demo_user"), &Secret::new("pw"))
        .await
        .unwrap();

    assert_eq!(status.expires_at, Some(1_767_225_600));
    assert_eq!(status.max_connections, Some(2));
    assert_eq!(status.active_connections, Some(1));
}

#[tokio::test]
async fn each_refusal_fixture_lands_in_its_own_taxonomy_arm() {
    for (name, expected) in [
        ("handshake-denied.json", AuthRejection::Credentials),
        ("handshake-expired.json", AuthRejection::Expired),
    ] {
        let stub = spawn_stub("200 OK", fixture(name), 1);
        let err = auth::authenticate(&client(), &stub.endpoint("u"), &Secret::new("pw"))
            .await
            .unwrap_err();
        match err {
            XtreamError::Unauthorized { rejection } => assert_eq!(rejection, expected, "{name}"),
            other => panic!("{name}: expected Unauthorized, got {other:?}"),
        }
    }
}

// ---- Failures that are the transport's, not the wire's --------------------------------

#[tokio::test]
async fn a_non_success_status_is_a_transport_failure_not_a_malformed_one() {
    let stub = spawn_stub("502 Bad Gateway", Vec::new(), 1);
    let err = auth::authenticate(&client(), &stub.endpoint("u"), &Secret::new("pw"))
        .await
        .unwrap_err();
    match err {
        XtreamError::Transport(inner) => assert_eq!(inner.to_string(), "source returned HTTP 502"),
        other => panic!("expected Transport, got {other:?}"),
    }
}

#[tokio::test]
async fn a_headend_answering_html_is_a_malformed_envelope() {
    // The classic misconfigured-panel response: a 200 with a login page in it.
    let stub = spawn_stub("200 OK", b"<html><body>Login</body></html>".to_vec(), 1);
    let err = auth::authenticate(&client(), &stub.endpoint("u"), &Secret::new("pw"))
        .await
        .unwrap_err();
    assert!(
        matches!(err, XtreamError::Malformed { .. }),
        "expected Malformed, got {err:?}"
    );
}

#[tokio::test]
async fn a_listing_that_answers_an_object_is_a_malformed_envelope() {
    // Some panels answer a listing action with the handshake block when the session lapses.
    let stub = spawn_stub("200 OK", fixture("handshake-denied.json"), 1);
    let err = catalog::live_streams(
        &client(),
        &stub.endpoint("u"),
        &Secret::new("pw"),
        &[],
        None,
    )
    .await
    .unwrap_err();
    match err {
        XtreamError::Malformed { detail } => {
            assert!(detail.contains("expected a list"), "detail was: {detail}");
        }
        other => panic!("expected Malformed, got {other:?}"),
    }
}

// ---- The corpus, end to end -------------------------------------------------------------

#[tokio::test]
async fn the_live_fixture_maps_its_good_rows_and_accounts_for_the_rest() {
    let categories = {
        let stub = spawn_stub("200 OK", fixture("live-categories.json"), 1);
        catalog::categories(
            &client(),
            &stub.endpoint("alice"),
            &Secret::new("pw"),
            MediaKind::Live,
        )
        .await
        .unwrap()
    };
    // Two of the six category rows are unusable (blank name, missing id).
    assert_eq!(categories.len(), 4);

    let stub = spawn_stub("200 OK", fixture("live-streams.json"), 1);
    let listing = catalog::live_streams(
        &client(),
        &stub.endpoint("alice"),
        &Secret::new("pw"),
        &categories,
        None,
    )
    .await
    .unwrap();

    let names: Vec<&str> = listing.channels.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "BBC One HD",
            "BBC One SD",
            "Sky Sports Main Event",
            "Channel With No Category",
            "Kids Channel",
        ]
    );

    let d = &listing.diagnostics;
    assert_eq!(d.total_seen(), 10);
    assert_eq!(d.emitted(), 5);
    assert_eq!(d.skipped(), 5);
    assert!(d.is_balanced());
    assert_eq!(d.skips_for(SkipReason::MissingId), 2, "empty id, zero id");
    assert_eq!(d.skips_for(SkipReason::MissingName), 2, "blank, absent");
    assert_eq!(d.skips_for(SkipReason::MalformedEntry), 1, "id is a list");

    // Category names became browse labels; the live default and the explicit override both
    // apply; and not one persisted locator carries a credential.
    assert_eq!(listing.channels[0].group_title.as_deref(), Some("News"));
    assert!(
        listing.channels[0]
            .locator
            .as_str()
            .ends_with("/live/4242.ts")
    );
    assert!(
        listing.channels[2]
            .locator
            .as_str()
            .ends_with("/live/9001.m3u8"),
        "an explicit container_extension must win over the live default"
    );
    assert_eq!(listing.channels[3].group_title, None, "blank category_id");
    for channel in &listing.channels {
        let locator = channel.locator.as_str();
        assert!(!locator.contains("alice"), "credential in {locator}");
        assert!(!locator.contains("pw"), "credential in {locator}");
    }
}

#[tokio::test]
async fn the_vod_fixture_resolves_its_containers() {
    let stub = spawn_stub("200 OK", fixture("vod-streams.json"), 1);
    let listing = catalog::vod_streams(
        &client(),
        &stub.endpoint("alice"),
        &Secret::new("pw"),
        &[],
        None,
    )
    .await
    .unwrap();

    let extensions: Vec<&str> = listing
        .channels
        .iter()
        .map(|c| c.locator.as_str().rsplit('.').next().unwrap())
        .collect();
    // mkv as given; MP4 normalized; blank → the VOD default; avi as given. The nonsense
    // container is skipped, not silently defaulted.
    assert_eq!(extensions, ["mkv", "mp4", "mp4", "avi"]);
    assert_eq!(
        listing.diagnostics.skips_for(SkipReason::UnusableExtension),
        1
    );
    assert!(listing.diagnostics.is_balanced());
    assert!(listing.channels.iter().all(|c| c.kind == MediaKind::Movie));
}

#[tokio::test]
async fn the_series_fixtures_expand_in_both_episode_shapes() {
    let stub = spawn_stub("200 OK", fixture("series.json"), 1);
    let shows = series::list(&client(), &stub.endpoint("alice"), &Secret::new("pw"), None)
        .await
        .unwrap();
    assert_eq!(shows.len(), 2, "two of the four rows are unusable");
    assert_eq!(shows[0].series_id, 55);
    assert_eq!(shows[1].series_id, 56, "a string series_id must read");

    // The documented keyed-object shape.
    let stub = spawn_stub("200 OK", fixture("series-info.json"), 1);
    let expansion = series::expand(&client(), &stub.endpoint("alice"), &Secret::new("pw"), 55)
        .await
        .unwrap();
    assert_eq!(
        stub.next_request(),
        "/player_api.php?username=alice&password=pw&action=get_series_info&series_id=55"
    );
    assert_eq!(expansion.series_name.as_deref(), Some("A Drama Series"));
    let placed: Vec<(u32, Option<u32>, &str)> = expansion
        .episodes
        .iter()
        .map(|e| (e.season, e.episode, e.channel.name.as_str()))
        .collect();
    assert_eq!(
        placed,
        [
            (1, Some(1), "Pilot"),
            (1, Some(2), "The Second One"),
            // A blank title earns a derived name rather than a skip, and `"info": []`
            // (PHP's empty object) must not cost the row.
            (1, Some(3), "S01E03"),
            // Keyed under "2" while its own field says season 0 — the key wins.
            (2, Some(1), "Return"),
        ]
    );
    assert_eq!(expansion.diagnostics.skips_for(SkipReason::MissingId), 1);
    assert!(expansion.diagnostics.is_balanced());
    assert_eq!(
        expansion.episodes[0].channel.logo.as_deref(),
        Some("http://cdn.example/stills/90001.jpg")
    );

    // The array shape: no season keys, so each episode's own field must carry it.
    let stub = spawn_stub("200 OK", fixture("series-info-array.json"), 1);
    let expansion = series::expand(&client(), &stub.endpoint("alice"), &Secret::new("pw"), 56)
        .await
        .unwrap();
    assert_eq!(expansion.series_name.as_deref(), Some("A Comedy Series"));
    let seasons: Vec<u32> = expansion.episodes.iter().map(|e| e.season).collect();
    assert_eq!(seasons, [1, 1, 2]);
    assert_eq!(expansion.diagnostics.skipped(), 0);
    assert!(
        expansion
            .episodes
            .iter()
            .all(|e| e.channel.kind == MediaKind::SeriesEpisode)
    );
}

// ---- The security property, on the live path ---------------------------------------------

#[tokio::test]
async fn nothing_the_catalog_hands_back_carries_a_credential() {
    // The end-to-end statement of §12: everything below is bound for SQLite and the FFI, so
    // rendering it whole must not surface the account. The playable URL exists only behind
    // an explicit `resolve_stream` at zap time.
    let password = "s3cr3t-passphrase";
    let stub = spawn_stub("200 OK", fixture("live-streams.json"), 1);
    let endpoint = stub.endpoint("alice");
    let listing = catalog::live_streams(&client(), &endpoint, &Secret::new(password), &[], None)
        .await
        .unwrap();

    let rendered = format!("{listing:?}");
    assert!(
        !rendered.contains(password),
        "the catalog leaked the password"
    );

    // And the resolved URL — the one value that does carry credentials — redacts itself.
    let stream = core_xtream::StreamRef::from_catalog_locator(&listing.channels[0].locator)
        .expect("a catalog locator must round-trip back to a stream reference");
    let resolved = endpoint
        .resolve_stream(&Secret::new(password), &stream)
        .unwrap();
    assert!(!format!("{resolved:?}").contains(password));
    assert!(
        resolved.into_locator().as_str().contains(password),
        "the resolved URL must be playable once deliberately unwrapped"
    );
}
