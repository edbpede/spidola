// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! `core-fetch`'s error taxonomy (TECH_SPEC §4.7, standing rule).
//!
//! Transport failures are classified into a small, matchable set so `core-api` can map
//! them onto the FFI error taxonomy (network-unreachable, unauthorized, …). The raw
//! `reqwest::Error` is preserved as the source for the log stream, never surfaced to the UI.

use thiserror::Error;

/// A fetch-layer failure.
#[derive(Debug, Error)]
pub enum FetchError {
    /// The HTTP client could not be constructed (bad config / TLS backend).
    #[error("failed to build the HTTP client")]
    Build(#[source] reqwest::Error),

    /// A request header name or value was invalid.
    #[error("invalid request header `{name}`: {reason}")]
    InvalidHeader {
        /// The offending header name.
        name: String,
        /// Why it was rejected (never contains secret material).
        reason: String,
    },

    /// The connection could not be established (DNS, refused, TLS).
    #[error("could not reach the source")]
    Connect(#[source] reqwest::Error),

    /// A timeout elapsed (connect, read, or the overall deadline).
    #[error("the request timed out")]
    Timeout(#[source] reqwest::Error),

    /// The redirect hop cap was exceeded.
    #[error("too many redirects")]
    TooManyRedirects(#[source] reqwest::Error),

    /// The source returned a non-success HTTP status.
    #[error("source returned HTTP {status}")]
    Status {
        /// The HTTP status code.
        status: u16,
    },

    /// Reading the response body failed mid-stream.
    #[error("error reading the response body")]
    Body(#[source] reqwest::Error),

    /// Any other request failure.
    #[error("request failed")]
    Request(#[source] reqwest::Error),
}

/// Result alias for the fetch layer.
pub type FetchResult<T> = Result<T, FetchError>;

/// Classifies a `reqwest::Error` into the matchable [`FetchError`] set.
pub(crate) fn classify(err: reqwest::Error) -> FetchError {
    if err.is_timeout() {
        FetchError::Timeout(err)
    } else if err.is_connect() {
        FetchError::Connect(err)
    } else if err.is_redirect() {
        FetchError::TooManyRedirects(err)
    } else if err.is_body() || err.is_decode() {
        FetchError::Body(err)
    } else {
        FetchError::Request(err)
    }
}
