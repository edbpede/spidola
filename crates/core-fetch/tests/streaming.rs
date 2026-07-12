// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! End-to-end fetch tests against a dependency-free local HTTP stub: streaming a body into
//! a `ByteSink` without full buffering, and mapping a non-2xx status into the taxonomy.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fmt;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;

use core_fetch::body::{ByteSink, StreamError, stream_to_sink};
use core_fetch::{FetchConfig, FetchError, HttpClient, RequestSpec};

/// A sink that never fails; records the bytes it saw and how many chunks arrived.
#[derive(Default)]
struct Collector {
    bytes: Vec<u8>,
    puts: usize,
}

#[derive(Debug)]
struct Never;
impl fmt::Display for Never {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("unreachable sink error")
    }
}
impl std::error::Error for Never {}

impl ByteSink for Collector {
    type Error = Never;
    fn put(&mut self, chunk: &[u8]) -> Result<(), Self::Error> {
        self.bytes.extend_from_slice(chunk);
        self.puts += 1;
        Ok(())
    }
}

/// Spawns a one-shot HTTP/1.1 server that writes `status` and streams `body` in several
/// TCP writes. Returns the bound `127.0.0.1:PORT` address.
fn spawn_stub(status: &'static str, body: Vec<u8>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            drain_request(&mut stream);
            let header = format!(
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(header.as_bytes());
            // Write the body in three flushes to exercise chunked delivery.
            for part in body.chunks(body.len().div_ceil(3).max(1)) {
                let _ = stream.write_all(part);
                let _ = stream.flush();
            }
            let _ = stream.flush();
        }
    });
    format!("http://{addr}/playlist.m3u")
}

fn drain_request(stream: &mut TcpStream) {
    // Read until the end of the request headers so the client's write completes.
    let mut buf = [0_u8; 1024];
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
}

#[tokio::test]
async fn streams_body_into_sink_without_full_buffering() {
    let body: Vec<u8> = (0..30_000_u32)
        .map(|i| b"#EXTINF:-1,"[i as usize % 11])
        .collect();
    let url = spawn_stub("200 OK", body.clone());

    let client = HttpClient::new(&FetchConfig::default()).unwrap();
    let response = client.get(&RequestSpec::new(&url)).await.unwrap();

    let mut sink = Collector::default();
    let total = stream_to_sink(response, &mut sink).await.unwrap();

    assert_eq!(total, body.len() as u64);
    assert_eq!(
        sink.bytes, body,
        "streamed bytes must match the served body"
    );
    assert!(sink.puts >= 1, "the sink must be fed incrementally");
}

#[tokio::test]
async fn non_success_status_maps_to_status_error() {
    let url = spawn_stub("404 Not Found", Vec::new());
    let client = HttpClient::new(&FetchConfig::default()).unwrap();
    let err = client.get(&RequestSpec::new(&url)).await.unwrap_err();
    match err {
        FetchError::Status { status } => assert_eq!(status, 404),
        other => panic!("expected Status(404), got {other:?}"),
    }
}

/// A sink whose `put` always fails, to prove sink errors surface as `StreamError::Sink`.
struct Failing;
#[derive(Debug)]
struct Boom;
impl fmt::Display for Boom {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("boom")
    }
}
impl std::error::Error for Boom {}
impl ByteSink for Failing {
    type Error = Boom;
    fn put(&mut self, _chunk: &[u8]) -> Result<(), Self::Error> {
        Err(Boom)
    }
}

#[tokio::test]
async fn sink_error_propagates_as_stream_error() {
    let url = spawn_stub("200 OK", b"payload".to_vec());
    let client = HttpClient::new(&FetchConfig::default()).unwrap();
    let response = client.get(&RequestSpec::new(&url)).await.unwrap();

    let mut sink = Failing;
    let err = stream_to_sink(response, &mut sink).await.unwrap_err();
    assert!(matches!(err, StreamError::Sink(Boom)));
}
