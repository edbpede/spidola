// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! `core-fetch` — all HTTP lives here (reqwest + rustls; no OpenSSL, TECH_SPEC §4.5).
//!
//! A per-source [`HttpClient`] carries the timeouts, redirect cap, TLS posture, and
//! user-agent; [`stream_to_sink`] streams response bodies into a caller's [`ByteSink`] with
//! no full buffering, so playlist bytes flow network → parser → DB batch and parser memory
//! stays bounded to one batch. The only sanctioned HTTP outside this crate is shell-side
//! artwork fetching (public logo URLs via platform image pipelines).
#![forbid(unsafe_code)]

pub mod body;
pub mod client;
pub mod error;
pub mod headers;
mod tls;

pub use body::{ByteSink, StreamError, stream_to_sink};
pub use client::{FetchConfig, HttpClient};
pub use error::{FetchError, FetchResult, classify};
pub use headers::RequestSpec;
