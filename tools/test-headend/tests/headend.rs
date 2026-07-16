// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::fs;
use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use spidola_test_headend::{Config, Headend, STREAM_FIXTURES};

static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

struct RunningHeadend {
    address: SocketAddr,
    shutdown: Option<Sender<()>>,
    server: Option<JoinHandle<io::Result<()>>>,
    assets_dir: PathBuf,
}

impl RunningHeadend {
    fn start() -> Self {
        let assets_dir = create_fixture_tree();
        let config = Config {
            assets_dir: assets_dir.clone(),
            public_base: String::new(),
            stall_duration: Duration::from_millis(150),
            drop_duration: Duration::ZERO,
        };
        config.validate_assets().expect("fixtures should validate");
        let headend = Headend::bind("127.0.0.1:0", config).expect("headend should bind");
        let address = headend
            .local_addr()
            .expect("headend should have an address");
        let (shutdown, shutdown_rx) = mpsc::channel();
        let server = thread::spawn(move || headend.serve(&shutdown_rx));
        Self {
            address,
            shutdown: Some(shutdown),
            server: Some(server),
            assets_dir,
        }
    }

    fn get(&self, path: &str) -> Vec<u8> {
        request(self.address, path, Duration::from_secs(2))
    }
}

impl Drop for RunningHeadend {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            shutdown.send(()).expect("server should receive shutdown");
        }
        if let Some(server) = self.server.take() {
            server
                .join()
                .expect("server thread should join")
                .expect("server should stop cleanly");
        }
        fs::remove_dir_all(&self.assets_dir).expect("temporary fixtures should be removed");
    }
}

#[test]
fn manifest_enumerates_success_and_failure_routes() {
    let server = RunningHeadend::start();
    let response = server.get("/manifest.json");
    assert_status(&response, 200);
    let body = String::from_utf8(response_body(&response).to_vec()).expect("manifest is UTF-8");
    for fixture in STREAM_FIXTURES {
        assert!(body.contains(fixture.id), "missing {}", fixture.id);
        assert!(body.contains(fixture.relative_path));
    }
    for route in [
        "unreachable",
        "unauthorized",
        "forbidden",
        "unsupported-format",
        "decoder-failed",
        "timeout",
        "unknown",
        "mid-stream-drop",
    ] {
        assert!(body.contains(route), "missing {route}");
    }
    assert!(body.contains(&format!("http://{}", server.address)));
}

#[test]
fn authorization_and_unreachable_routes_have_deterministic_http_semantics() {
    let server = RunningHeadend::start();

    let unreachable = server.get("/unreachable");
    assert_status(&unreachable, 302);
    assert!(headers(&unreachable).contains("Location: http://spidola.invalid/unreachable\r\n"));

    let unauthorized = server.get("/unauthorized");
    assert_status(&unauthorized, 401);
    assert!(headers(&unauthorized).contains("WWW-Authenticate: Basic realm=\"spidola-test\""));

    assert_status(&server.get("/forbidden"), 403);
    assert_status(&server.get("/unknown"), 520);
}

#[test]
fn unsupported_format_is_an_archive_disguised_as_transport_stream() {
    let server = RunningHeadend::start();
    let response = server.get("/unsupported-format");
    assert_status(&response, 200);
    assert!(headers(&response).contains("Content-Type: video/mp2t"));
    assert!(response_body(&response).starts_with(b"PK\x03\x04"));
}

#[test]
fn decoder_failure_preserves_the_lead_then_corrupts_transport_packets() {
    let server = RunningHeadend::start();
    let original = fs::read(server.assets_dir.join("ts-h264-aac.ts")).expect("read source fixture");
    let response = server.get("/decoder-failed");
    assert_status(&response, 200);
    let body = response_body(&response);
    assert_eq!(body.len(), original.len());
    assert_eq!(&body[..300], &original[..300]);
    assert_ne!(&body[500..], &original[500..]);
}

#[test]
fn timeout_sends_complete_headers_without_a_body() {
    let server = RunningHeadend::start();
    let response = request(server.address, "/timeout", Duration::from_millis(50));
    assert_status(&response, 200);
    assert_eq!(response_body(&response), b"");
    assert!(headers(&response).contains("Content-Length: 16777216"));
}

#[test]
fn mid_stream_drop_closes_before_the_declared_content_length() {
    let server = RunningHeadend::start();
    let response = server.get("/mid-stream-drop");
    assert_status(&response, 200);
    let declared = content_length(&response);
    assert!(declared > response_body(&response).len());
    assert!(!response_body(&response).is_empty());
}

#[test]
fn assets_are_served_with_media_types_and_traversal_is_rejected() {
    let server = RunningHeadend::start();
    let playlist = server.get("/streams/hls-h264-aac/master.m3u8");
    assert_status(&playlist, 200);
    assert!(headers(&playlist).contains("Content-Type: application/vnd.apple.mpegurl"));
    assert_eq!(response_body(&playlist), b"fixture:hls-h264-aac");

    assert_status(&server.get("/streams/../ts-h264-aac.ts"), 400);
}

#[test]
fn assets_support_single_byte_ranges_for_seekable_players() {
    let server = RunningHeadend::start();
    let response = request_with_headers(
        server.address,
        "/streams/hls-h264-aac/master.m3u8",
        &["Range: bytes=8-15"],
        Duration::from_secs(2),
    );
    assert_status(&response, 206);
    assert!(headers(&response).contains("Accept-Ranges: bytes"));
    assert!(headers(&response).contains("Content-Range: bytes 8-15/20"));
    assert_eq!(response_body(&response), b"hls-h264");

    let invalid = request_with_headers(
        server.address,
        "/streams/hls-h264-aac/master.m3u8",
        &["Range: bytes=100-200"],
        Duration::from_secs(2),
    );
    assert_status(&invalid, 416);
    assert!(headers(&invalid).contains("Content-Range: bytes */20"));
}

fn create_fixture_tree() -> PathBuf {
    let unique = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "spidola-test-headend-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&root).expect("create temporary fixture root");
    for fixture in STREAM_FIXTURES {
        let path = root.join(fixture.relative_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create fixture directory");
        }
        let body = if fixture.id == "ts-h264-aac" {
            synthetic_transport_stream()
        } else {
            format!("fixture:{}", fixture.id).into_bytes()
        };
        fs::write(path, body).expect("write fixture");
    }
    root
}

fn synthetic_transport_stream() -> Vec<u8> {
    let mut stream = Vec::with_capacity(188 * 6);
    for packet_index in 0_u8..6 {
        let mut packet = vec![packet_index; 188];
        packet[0] = 0x47;
        packet[1] = 0x01;
        packet[2] = 0x00;
        packet[3] = 0x10;
        stream.extend(packet);
    }
    stream
}

fn request(address: SocketAddr, path: &str, timeout: Duration) -> Vec<u8> {
    request_with_headers(address, path, &[], timeout)
}

fn request_with_headers(
    address: SocketAddr,
    path: &str,
    request_headers: &[&str],
    timeout: Duration,
) -> Vec<u8> {
    let mut stream = TcpStream::connect(address).expect("connect to headend");
    stream.set_read_timeout(Some(timeout)).expect("set timeout");
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n"
    )
    .expect("write request");
    for header in request_headers {
        write!(stream, "{header}\r\n").expect("write request header");
    }
    write!(stream, "\r\n").expect("finish request headers");
    stream
        .shutdown(std::net::Shutdown::Write)
        .expect("finish request");

    let mut response = Vec::new();
    let mut chunk = [0_u8; 1024];
    loop {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(read) => response.extend_from_slice(&chunk[..read]),
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                break;
            }
            Err(error) => panic!("read response: {error}"),
        }
    }
    response
}

fn assert_status(response: &[u8], expected: u16) {
    let line_end = response
        .windows(2)
        .position(|window| window == b"\r\n")
        .expect("response status line");
    let status = std::str::from_utf8(&response[..line_end]).expect("status is UTF-8");
    assert!(
        status.starts_with(&format!("HTTP/1.1 {expected} ")),
        "unexpected status: {status}"
    );
}

fn headers(response: &[u8]) -> &str {
    let body_start = body_start(response);
    std::str::from_utf8(&response[..body_start]).expect("headers are UTF-8")
}

fn response_body(response: &[u8]) -> &[u8] {
    &response[body_start(response)..]
}

fn body_start(response: &[u8]) -> usize {
    response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|index| index + 4)
        .expect("response header terminator")
}

fn content_length(response: &[u8]) -> usize {
    headers(response)
        .lines()
        .find_map(|line| line.strip_prefix("Content-Length: "))
        .expect("content length header")
        .parse()
        .expect("numeric content length")
}
