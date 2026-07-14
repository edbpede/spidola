// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! `core-xtream` — typed Xtream Codes client; nothing Xtream-shaped leaks upward.
//!
//! A client over the Xtream Codes HTTP API: the [`auth`] handshake, the [`catalog`]
//! listings, and [`series`] expansion, all mapped into `core-model` values (TECH_SPEC
//! §4.3). Two rules give the crate its shape:
//!
//! - **The wire stays inside.** Xtream's responses are inconsistent by nature — numbers as
//!   strings, missing fields, `""` for null, four spellings of a boolean — so the `wire`
//!   module reads them defensively and is private. Callers see domain rows, never DTOs, and
//!   a headend's quirks stop at this boundary.
//! - **Credentials touch exactly one module.** Xtream puts the account password *in the
//!   URL*, so every URL that carries one is built by [`urls`] and nowhere else. That module
//!   is the audited point §12 requires; read its header before changing anything about how
//!   a password reaches a request.
//!
//! Tolerance mirrors `core-parse` (§4.2): a malformed *entry* is skipped and counted in
//! [`Diagnostics`], never escalated; a malformed *envelope* is an [`XtreamError`].
//!
//! HTTP is `core-fetch`'s, always: this crate constructs no client and speaks no transport
//! of its own (§4.5).
#![forbid(unsafe_code)]

pub mod auth;
pub mod catalog;
pub mod diagnostics;
pub mod epg;
pub mod error;
pub mod request;
pub mod series;
pub mod urls;
mod wire;

pub use auth::{AccountStatus, authenticate};
pub use catalog::{CatalogCategory, CatalogChannel, Listing};
pub use diagnostics::{Diagnostics, SkipReason};
pub use error::{AuthRejection, XtreamError, XtreamResult};
pub use series::{EpisodeRow, Expansion, Show};
pub use urls::{CredentialUrl, Endpoint, ResolvedStream, StreamRef};

/// This crate's `tracing` target, following the target-per-subsystem convention
/// (TECH_SPEC §4.8) that the shells map onto one native logging category each.
///
/// Spelled literally rather than referencing `core-api`'s `logging::targets`, because the
/// dependency runs the other way — `core-api` composes this crate, not the reverse (§3.1,
/// depend downward). That constant set should grow a matching `XTREAM` entry when
/// `core-api` wires this crate up.
pub const LOG_TARGET: &str = "spidola::xtream";
