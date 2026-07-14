// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! `services` — one file per service (paged-by-contract read paths, TECH_SPEC §4.6).
//!
//! Each service is a UniFFI object composed by [`crate::Core`] over the shared runtime and
//! database handles. `catalog`, `epg`, and `pairing` cover the wider surface; `epg` and
//! `pairing` stay Phase-0 stubs until Phases 8 and 6 respectively.
//! (This module cannot `#![forbid(unsafe_code)]` — it hosts `#[uniffi::export]` impls whose
//! generated FFI scaffolding contains `unsafe`; the workspace warns on `unsafe` instead.)
pub mod catalog;
pub mod epg;
pub mod favorites;
pub mod pairing;
pub mod recents;
pub mod search;
pub mod settings;
pub mod source;

pub use catalog::CatalogService;
pub use favorites::FavoritesService;
pub use recents::RecentsService;
pub use search::SearchService;
pub use settings::SettingsService;
pub use source::SourceService;
