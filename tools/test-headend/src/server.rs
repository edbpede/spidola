// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fs;
use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::thread;
use std::time::Duration;

const MAX_REQUEST_BYTES: usize = 8 * 1024;
const TS_PACKET_BYTES: usize = 188;
const FIXTURE_VIDEO_PID: u16 = 0x100;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StreamFixture {
    pub id: &'static str,
    pub relative_path: &'static str,
}

pub const STREAM_FIXTURES: [StreamFixture; 7] = [
    StreamFixture {
        id: "hls-h264-aac",
        relative_path: "hls-h264-aac/master.m3u8",
    },
    StreamFixture {
        id: "hls-hevc-eac3",
        relative_path: "hls-hevc-eac3/master.m3u8",
    },
    StreamFixture {
        id: "dash-h264-aac",
        relative_path: "dash-h264-aac/manifest.mpd",
    },
    StreamFixture {
        id: "ts-mpeg2-mp2",
        relative_path: "ts-mpeg2-mp2.ts",
    },
    StreamFixture {
        id: "ts-h264-aac",
        relative_path: "ts-h264-aac.ts",
    },
    StreamFixture {
        id: "mkv-vp9-opus",
        relative_path: "mkv-vp9-opus.mkv",
    },
    StreamFixture {
        id: "hls-multi-audio-subs",
        relative_path: "hls-multi-audio-subs/master.m3u8",
    },
];

#[derive(Clone, Debug)]
pub struct Config {
    pub assets_dir: PathBuf,
    pub public_base: String,
    pub stall_duration: Duration,
    pub drop_duration: Duration,
}

impl Config {
    /// Confirms that every acceptance-stream entry point exists beneath the asset directory.
    ///
    /// # Errors
    ///
    /// Returns [`io::ErrorKind::NotFound`] with the missing fixture name and generation command
    /// when any required entry point is absent.
    pub fn validate_assets(&self) -> io::Result<()> {
        for fixture in STREAM_FIXTURES {
            let path = self.assets_dir.join(fixture.relative_path);
            if !path.is_file() {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!(
                        "missing {} fixture at {}; run tools/test-headend/headend.sh generate",
                        fixture.id,
                        path.display()
                    ),
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct Headend {
    listener: TcpListener,
    config: Config,
}

impl Headend {
    /// Binds the headend listener and derives its public URL when none was configured.
    ///
    /// # Errors
    ///
    /// Returns an I/O error when the address cannot be bound, nonblocking mode cannot be set, or
    /// the listener's local address cannot be read.
    pub fn bind(address: impl ToSocketAddrs, mut config: Config) -> io::Result<Self> {
        let listener = TcpListener::bind(address)?;
        listener.set_nonblocking(true)?;
        if config.public_base.is_empty() {
            config.public_base = format!("http://{}", listener.local_addr()?);
        }
        Ok(Self { listener, config })
    }

    /// Returns the socket address currently owned by the listener.
    ///
    /// # Errors
    ///
    /// Returns an I/O error when the operating system cannot report the listener address.
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.listener.local_addr()
    }

    /// Serves connections until a shutdown message arrives or every sender is dropped.
    ///
    /// # Errors
    ///
    /// Returns an I/O error when accepting a connection fails for a reason other than a
    /// nonblocking listener having no pending connection.
    pub fn serve(self, shutdown: &Receiver<()>) -> io::Result<()> {
        loop {
            match shutdown.try_recv() {
                Ok(()) | Err(TryRecvError::Disconnected) => return Ok(()),
                Err(TryRecvError::Empty) => {}
            }

            match self.listener.accept() {
                Ok((stream, _peer)) => {
                    let config = self.config.clone();
                    drop(thread::spawn(move || {
                        if let Err(error) = handle_connection(stream, &config) {
                            eprintln!("test headend request failed: {error}");
                        }
                    }));
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(error) => return Err(error),
            }
        }
    }
}

fn handle_connection(mut stream: TcpStream, config: &Config) -> io::Result<()> {
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    let request = read_request(&mut stream)?;
    let Some((method, target)) = request_line(&request) else {
        return write_text(&mut stream, 400, "Bad Request", "malformed request\n", &[]);
    };
    if method != "GET" {
        return write_text(
            &mut stream,
            405,
            "Method Not Allowed",
            "only GET is supported\n",
            &[("Allow", "GET")],
        );
    }

    let path = target.split('?').next().unwrap_or(target);
    let requested_range = header_value(&request, "Range");
    match path {
        "/" => serve_index(&mut stream, config),
        "/manifest.json" => serve_manifest(&mut stream, config),
        "/unreachable" => write_text(
            &mut stream,
            302,
            "Found",
            "redirecting to an intentionally unresolvable host\n",
            &[("Location", "http://spidola.invalid/unreachable")],
        ),
        "/unauthorized" => write_text(
            &mut stream,
            401,
            "Unauthorized",
            "credentials required\n",
            &[("WWW-Authenticate", "Basic realm=\"spidola-test\"")],
        ),
        "/forbidden" => write_text(&mut stream, 403, "Forbidden", "access denied\n", &[]),
        "/unsupported-format" => serve_unsupported(&mut stream),
        "/decoder-failed" => serve_decoder_failure(&mut stream, config),
        "/timeout" => serve_timeout(&mut stream, config),
        "/mid-stream-drop" => serve_mid_stream_drop(&mut stream, config),
        "/unknown" => write_text(
            &mut stream,
            520,
            "Unknown Error",
            "intentionally uncategorized headend failure\n",
            &[("X-Spidola-Expected-Error", "Unknown")],
        ),
        _ if path.starts_with("/streams/") => {
            serve_asset(&mut stream, config, path, requested_range)
        }
        _ => write_text(&mut stream, 404, "Not Found", "route not found\n", &[]),
    }
}

fn read_request(stream: &mut TcpStream) -> io::Result<Vec<u8>> {
    let mut request = Vec::with_capacity(1024);
    let mut chunk = [0_u8; 512];
    while request.len() < MAX_REQUEST_BYTES {
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        request.extend_from_slice(&chunk[..read]);
        if request.windows(4).any(|window| window == b"\r\n\r\n") {
            return Ok(request);
        }
    }
    if request.len() >= MAX_REQUEST_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "request headers exceed 8 KiB",
        ));
    }
    Ok(request)
}

fn request_line(request: &[u8]) -> Option<(&str, &str)> {
    let line_end = request.windows(2).position(|window| window == b"\r\n")?;
    let line = std::str::from_utf8(&request[..line_end]).ok()?;
    let mut parts = line.split_ascii_whitespace();
    let method = parts.next()?;
    let target = parts.next()?;
    let version = parts.next()?;
    if parts.next().is_some() || !version.starts_with("HTTP/1.") {
        return None;
    }
    Some((method, target))
}

fn header_value<'a>(request: &'a [u8], requested_name: &str) -> Option<&'a str> {
    let request = std::str::from_utf8(request).ok()?;
    request.lines().skip(1).find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case(requested_name)
            .then(|| value.trim())
    })
}

fn serve_index(stream: &mut TcpStream, config: &Config) -> io::Result<()> {
    let manifest_url = format!("{}/manifest.json", config.public_base.trim_end_matches('/'));
    let body = format!(
        "<!doctype html><title>Spidola test headend</title>\
         <h1>Spidola test headend</h1>\
         <p>Repository-owned synthetic acceptance media only.</p>\
         <p><a href=\"{manifest_url}\">Route manifest</a></p>"
    );
    write_response(
        stream,
        200,
        "OK",
        "text/html; charset=utf-8",
        body.as_bytes(),
        &[],
    )
}

fn serve_manifest(stream: &mut TcpStream, config: &Config) -> io::Result<()> {
    let base = json_escape(config.public_base.trim_end_matches('/'));
    let streams = STREAM_FIXTURES
        .iter()
        .map(|fixture| {
            format!(
                "{{\"id\":\"{}\",\"url\":\"{base}/streams/{}\"}}",
                fixture.id, fixture.relative_path
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let body = format!(
        "{{\"streams\":[{streams}],\"failures\":{{\
         \"SourceUnreachable\":\"{base}/unreachable\",\
         \"Unauthorized\":\"{base}/unauthorized\",\
         \"Forbidden\":\"{base}/forbidden\",\
         \"UnsupportedFormat\":\"{base}/unsupported-format\",\
         \"DecoderFailed\":\"{base}/decoder-failed\",\
         \"Timeout\":\"{base}/timeout\",\
         \"Unknown\":\"{base}/unknown\",\
         \"MidStreamDrop\":\"{base}/mid-stream-drop\"}}}}\n"
    );
    write_response(
        stream,
        200,
        "OK",
        "application/json; charset=utf-8",
        body.as_bytes(),
        &[("Cache-Control", "no-store")],
    )
}

fn json_escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            character => escaped.push(character),
        }
    }
    escaped
}

fn serve_asset(
    stream: &mut TcpStream,
    config: &Config,
    request_path: &str,
    requested_range: Option<&str>,
) -> io::Result<()> {
    let relative = request_path.trim_start_matches("/streams/");
    if relative.is_empty()
        || relative.split('/').any(|component| component == "..")
        || relative.contains('\\')
    {
        return write_text(stream, 400, "Bad Request", "invalid asset path\n", &[]);
    }
    let path = config.assets_dir.join(relative);
    if !path.is_file() || !path.starts_with(&config.assets_dir) {
        return write_text(stream, 404, "Not Found", "asset not found\n", &[]);
    }
    let body = fs::read(&path)?;
    let Some(requested_range) = requested_range else {
        return write_response(
            stream,
            200,
            "OK",
            content_type(&path),
            &body,
            &[("Accept-Ranges", "bytes")],
        );
    };
    let Some((start, end)) = parse_byte_range(requested_range, body.len()) else {
        let content_range = format!("bytes */{}", body.len());
        return write_response(
            stream,
            416,
            "Range Not Satisfiable",
            "text/plain; charset=utf-8",
            b"requested byte range is not satisfiable\n",
            &[
                ("Accept-Ranges", "bytes"),
                ("Content-Range", &content_range),
            ],
        );
    };
    let content_range = format!("bytes {start}-{}/{}", end - 1, body.len());
    write_response(
        stream,
        206,
        "Partial Content",
        content_type(&path),
        &body[start..end],
        &[
            ("Accept-Ranges", "bytes"),
            ("Content-Range", &content_range),
        ],
    )
}

fn parse_byte_range(value: &str, content_length: usize) -> Option<(usize, usize)> {
    let range = value.strip_prefix("bytes=")?;
    if content_length == 0 || range.contains(',') {
        return None;
    }
    let (start, end) = range.split_once('-')?;
    if start.is_empty() {
        let suffix_length = end.parse::<usize>().ok()?;
        if suffix_length == 0 {
            return None;
        }
        return Some((content_length.saturating_sub(suffix_length), content_length));
    }

    let start = start.parse::<usize>().ok()?;
    if start >= content_length {
        return None;
    }
    let end = if end.is_empty() {
        content_length
    } else {
        let inclusive_end = end.parse::<usize>().ok()?;
        if inclusive_end < start {
            return None;
        }
        inclusive_end.saturating_add(1).min(content_length)
    };
    Some((start, end))
}

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("m3u8") => "application/vnd.apple.mpegurl",
        Some("m4s") => "video/iso.segment",
        Some("mp4") => "video/mp4",
        Some("mpd") => "application/dash+xml",
        Some("ts") => "video/mp2t",
        Some("mkv") => "video/x-matroska",
        Some("vtt") => "text/vtt; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn serve_unsupported(stream: &mut TcpStream) -> io::Result<()> {
    const RENAMED_ARCHIVE: &[u8] = b"PK\x03\x04not-a-media-container\x00\x01\x02\x03";
    write_response(
        stream,
        200,
        "OK",
        "video/mp2t",
        RENAMED_ARCHIVE,
        &[("X-Spidola-Expected-Error", "UnsupportedFormat")],
    )
}

fn serve_decoder_failure(stream: &mut TcpStream, config: &Config) -> io::Result<()> {
    let path = config.assets_dir.join("ts-h264-aac.ts");
    let mut body = fs::read(&path)?;
    if body.len() < TS_PACKET_BYTES * 3 {
        return write_text(
            stream,
            503,
            "Service Unavailable",
            "ts-h264-aac fixture is too short; regenerate the assets\n",
            &[],
        );
    }
    let approximate_third = body.len() / 3;
    let corrupt_from = approximate_third - (approximate_third % TS_PACKET_BYTES);
    let corrupted_packets = body[corrupt_from..]
        .chunks_exact_mut(TS_PACKET_BYTES)
        .filter(|packet| transport_stream_pid(packet) == Some(FIXTURE_VIDEO_PID))
        .filter_map(transport_stream_payload_mut)
        .map(|payload| {
            for byte in payload {
                *byte ^= 0xa5;
            }
        })
        .count();
    if corrupted_packets == 0 {
        return write_text(
            stream,
            503,
            "Service Unavailable",
            "ts-h264-aac fixture does not contain the expected video PID; regenerate the assets\n",
            &[],
        );
    }
    write_response(
        stream,
        200,
        "OK",
        "video/mp2t",
        &body,
        &[("X-Spidola-Expected-Error", "DecoderFailed")],
    )
}

fn transport_stream_pid(packet: &[u8]) -> Option<u16> {
    if packet.len() != TS_PACKET_BYTES || packet[0] != 0x47 {
        return None;
    }
    Some((u16::from(packet[1] & 0x1f) << 8) | u16::from(packet[2]))
}

fn transport_stream_payload_mut(packet: &mut [u8]) -> Option<&mut [u8]> {
    let adaptation_control = (packet[3] >> 4) & 0x03;
    let payload_start = match adaptation_control {
        1 => 4,
        3 => 5 + usize::from(packet[4]),
        _ => return None,
    };
    (payload_start < packet.len()).then(|| &mut packet[payload_start..])
}

fn serve_timeout(stream: &mut TcpStream, config: &Config) -> io::Result<()> {
    write_headers(
        stream,
        200,
        "OK",
        "video/mp2t",
        16 * 1024 * 1024,
        &[("X-Spidola-Expected-Error", "Timeout")],
    )?;
    stream.flush()?;
    thread::sleep(config.stall_duration);
    Ok(())
}

fn serve_mid_stream_drop(stream: &mut TcpStream, config: &Config) -> io::Result<()> {
    let body = fs::read(config.assets_dir.join("ts-h264-aac.ts"))?;
    if body.is_empty() {
        return write_text(
            stream,
            503,
            "Service Unavailable",
            "ts-h264-aac fixture is empty; regenerate the assets\n",
            &[],
        );
    }
    let partial_len = (body.len() / 3).max(1);
    write_headers(
        stream,
        200,
        "OK",
        "video/mp2t",
        body.len(),
        &[("X-Spidola-Expected-Error", "MidStreamDrop")],
    )?;
    let chunks = body[..partial_len]
        .chunks(TS_PACKET_BYTES * 64)
        .collect::<Vec<_>>();
    let pause = if chunks.is_empty() {
        Duration::ZERO
    } else {
        config.drop_duration / u32::try_from(chunks.len()).unwrap_or(u32::MAX)
    };
    for chunk in chunks {
        stream.write_all(chunk)?;
        stream.flush()?;
        thread::sleep(pause);
    }
    Ok(())
}

fn write_text(
    stream: &mut TcpStream,
    status: u16,
    reason: &str,
    body: &str,
    headers: &[(&str, &str)],
) -> io::Result<()> {
    write_response(
        stream,
        status,
        reason,
        "text/plain; charset=utf-8",
        body.as_bytes(),
        headers,
    )
}

fn write_response(
    stream: &mut TcpStream,
    status: u16,
    reason: &str,
    content_type: &str,
    body: &[u8],
    headers: &[(&str, &str)],
) -> io::Result<()> {
    write_headers(stream, status, reason, content_type, body.len(), headers)?;
    stream.write_all(body)
}

fn write_headers(
    stream: &mut TcpStream,
    status: u16,
    reason: &str,
    content_type: &str,
    content_length: usize,
    headers: &[(&str, &str)],
) -> io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {content_length}\r\nConnection: close\r\n"
    )?;
    for (name, value) in headers {
        write!(stream, "{name}: {value}\r\n")?;
    }
    write!(stream, "\r\n")
}
