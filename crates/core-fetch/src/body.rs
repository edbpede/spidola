// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Streaming body → sink adapter — no full buffering (TECH_SPEC §4.5).
//!
//! Playlist bytes flow network → parser → DB batch without ever materializing the whole
//! response: [`stream_to_sink`] pulls the body one chunk at a time and hands each chunk to
//! a caller-provided [`ByteSink`] (in practice the M3U parser's push interface, wired at
//! the orchestration layer so `core-fetch` stays ignorant of `core-parse`). This is what
//! keeps parser memory bounded to one batch regardless of playlist size.

use thiserror::Error;

use crate::error::{FetchError, classify};

/// A push target for streamed response bytes.
///
/// Implementors carry their own error (e.g. a parser or storage failure). The sink is fed
/// incrementally and must not assume it has seen the whole body until the stream ends.
pub trait ByteSink {
    /// The sink's own failure type.
    type Error: std::error::Error + 'static;

    /// Consumes one chunk of body bytes.
    ///
    /// # Errors
    /// Returns the sink's own error to abort the stream (propagated as
    /// [`StreamError::Sink`]).
    fn put(&mut self, chunk: &[u8]) -> Result<(), Self::Error>;
}

/// Either the transport failed or the sink did.
#[derive(Debug, Error)]
pub enum StreamError<E: std::error::Error + 'static> {
    /// The HTTP transport failed mid-stream.
    #[error("fetch failed while streaming the body")]
    Fetch(#[source] FetchError),
    /// The sink rejected a chunk.
    #[error("sink failed while consuming the body")]
    Sink(#[source] E),
}

/// Streams the response body into `sink`, returning the total byte count.
///
/// # Errors
/// Returns [`StreamError::Fetch`] on a transport failure or [`StreamError::Sink`] if the
/// sink aborts.
pub async fn stream_to_sink<S: ByteSink>(
    mut response: reqwest::Response,
    sink: &mut S,
) -> Result<u64, StreamError<S::Error>> {
    let mut total: u64 = 0;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| StreamError::Fetch(classify(e)))?
    {
        total += chunk.len() as u64;
        sink.put(&chunk).map_err(StreamError::Sink)?;
    }
    Ok(total)
}
