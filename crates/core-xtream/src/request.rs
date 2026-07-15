// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Issuing one `player_api.php` request and collecting its body (TECH_SPEC §4.3, §4.5).
//!
//! Every Xtream call in the crate goes through [`get`]: it asks `crate::urls` for the
//! audited credential URL, hands it to `core-fetch` (the only HTTP in the project), and
//! streams the response into a capped buffer.
//!
//! **Why this buffers where the M3U path streams.** `core-parse` never materializes a
//! playlist because M3U is line-oriented and can be parsed incrementally (§4.2). Xtream
//! cannot: a catalog arrives as one JSON array with no pagination and no envelope to
//! resume from, so the body must be whole before it can be read. [`MAX_BODY_BYTES`] is the
//! honest consequence — a ceiling that keeps a broken or hostile headend from exhausting
//! memory on a 1 GB device, chosen with enough slack that no real catalog reaches it.

use core_fetch::body::{ByteSink, StreamError, stream_to_sink};
use core_fetch::{HttpClient, RequestSpec};
use core_model::secret::Secret;

use crate::LOG_TARGET;
use crate::error::{XtreamError, XtreamResult};
use crate::urls::Endpoint;

/// The largest response body the client will accept.
///
/// A 50k-title VOD catalog — the PRD's ceiling — serializes to roughly 20 MB, so 64 MiB is
/// several times the worst honest case while still bounding the damage a runaway headend
/// can do.
pub const MAX_BODY_BYTES: usize = 64 * 1024 * 1024;

/// The cap must stay well clear of the largest honest catalog, or it stops being a backstop
/// and starts being a bug. Asserted at compile time so lowering it "just a little" fails the
/// build rather than a user's import.
const _: () = assert!(MAX_BODY_BYTES > 32 * 1024 * 1024);

/// The response body exceeded [`MAX_BODY_BYTES`] and was abandoned mid-stream.
///
/// A sink error rather than a check after the fact, so an oversized body is dropped as it
/// arrives instead of being buffered first and rejected afterwards.
#[derive(Debug)]
struct BodyTooLarge;

impl std::fmt::Display for BodyTooLarge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "response body exceeded {MAX_BODY_BYTES} bytes")
    }
}

impl std::error::Error for BodyTooLarge {}

/// Accumulates the response body, refusing to grow past [`MAX_BODY_BYTES`].
#[derive(Default)]
struct BodyBuffer {
    bytes: Vec<u8>,
}

impl ByteSink for BodyBuffer {
    type Error = BodyTooLarge;

    fn put(&mut self, chunk: &[u8]) -> Result<(), Self::Error> {
        if self.bytes.len() + chunk.len() > MAX_BODY_BYTES {
            return Err(BodyTooLarge);
        }
        self.bytes.extend_from_slice(chunk);
        Ok(())
    }
}

/// Issues one `player_api.php` GET with `params` (typically `action=…`) and returns the
/// response body.
///
/// `password` is borrowed only for the URL construction inside `crate::urls`; the resulting
/// credential URL is exposed exactly once, to `core-fetch`, and is never logged — the
/// `action` fields below are the whole of what reaches the log stream (§4.8).
///
/// # Errors
/// Returns [`XtreamError::Transport`] if the request fails or the headend answers non-2xx,
/// [`XtreamError::ResponseTooLarge`] if the body exceeds [`MAX_BODY_BYTES`], or
/// [`XtreamError::InvalidServer`] if the account's base URL cannot host the API.
pub(crate) async fn get(
    http: &HttpClient,
    endpoint: &Endpoint,
    password: &Secret,
    params: &[(&str, &str)],
) -> XtreamResult<Vec<u8>> {
    let action = params
        .iter()
        .find_map(|(key, value)| (*key == "action").then_some(*value))
        .unwrap_or("handshake");
    tracing::debug!(target: LOG_TARGET, action, "issuing an Xtream request");

    let url = endpoint.player_api(password, params)?;
    let response = http.get(&RequestSpec::new(url.expose())).await?;
    drop(url);

    let mut buffer = BodyBuffer::default();
    let bytes = stream_to_sink(response, &mut buffer)
        .await
        .map_err(|e| match e {
            StreamError::Fetch(fetch) => XtreamError::Transport(fetch),
            StreamError::Sink(BodyTooLarge) => XtreamError::ResponseTooLarge {
                limit: MAX_BODY_BYTES,
            },
        })?;
    tracing::debug!(target: LOG_TARGET, action, bytes, "Xtream response received");
    Ok(buffer.bytes)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn the_buffer_refuses_to_grow_past_the_cap() {
        let mut buffer = BodyBuffer::default();
        assert!(buffer.put(b"hello").is_ok());
        // One chunk that would cross the cap is refused whole — nothing is partially kept.
        let oversized = vec![0_u8; MAX_BODY_BYTES];
        assert!(buffer.put(&oversized).is_err());
        assert_eq!(buffer.bytes, b"hello", "a refused chunk must not be stored");
    }
}
