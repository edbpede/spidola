// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! `core-parse` — streaming M3U and XMLTV parsers; pure, bounded-memory, property-tested.
//!
//! Parsers consume the input as chunks and emit channels in batches through a caller
//! [`ChannelSink`], never buffering the whole playlist (TECH_SPEC §4.2). They are pure (no
//! I/O, no clock beyond an injected "now" for the EPG window) and tolerant (unknown
//! attributes preserved, malformed entries skipped-and-counted). The XMLTV parser (same
//! streaming shape) lands in Phase 8.
#![forbid(unsafe_code)]

pub mod error;
pub mod m3u;
pub mod sink;
pub mod xmltv;

pub use error::ParseError;
pub use m3u::diagnostics::{Diagnostics, SkipReason};
pub use m3u::{DEFAULT_BATCH_SIZE, M3uParser};
pub use sink::{ChannelSink, ParsedChannel};
