// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! `core-model` — domain types; illegal states must not construct (parse, don't validate).
//!
//! Plain, serde-friendly data types with a deliberate, re-exported public surface
//! (TECH_SPEC §4.1). Identifiers are newtypes so cross-wiring is a compile error; secrets
//! redact and zeroize themselves and never serialize their raw value (§12); the stream
//! locator only exists once it has parsed. This crate is pure: no I/O, no clocks (times are
//! Unix seconds), so it is trivially unit-testable.
#![forbid(unsafe_code)]

pub mod category;
pub mod channel;
pub mod epg;
pub mod error;
pub mod favorite;
pub mod history;
pub mod ids;
pub mod locator;
pub mod secret;
pub mod source;

pub use category::Category;
pub use channel::{Channel, ChannelOverrides, MediaKind, channel_identity};
pub use epg::EpgEntry;
pub use error::ModelError;
pub use favorite::Favorite;
pub use history::PlaybackHistoryEntry;
pub use ids::{CategoryId, ChannelId, ChannelIdentity, EpgEntryId, HistoryId, SecretRef, SourceId};
pub use locator::StreamLocator;
pub use secret::Secret;
pub use source::{Source, SourceCommon, SourceKind};
