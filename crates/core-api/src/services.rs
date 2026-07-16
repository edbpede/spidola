// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! `services` — one file per service (paged-by-contract read paths, TECH_SPEC §4.6).
//!
//! Each service is a UniFFI object composed by [`crate::Core`] over the shared runtime and
//! database handles. `epg` stays a Phase-0 stub until Phase 8, when EPG ingest lands.
//! (This module cannot `#![forbid(unsafe_code)]` — it hosts `#[uniffi::export]` impls whose
//! generated FFI scaffolding contains `unsafe`; the workspace warns on `unsafe` instead.)
pub mod catalog;
pub mod custom;
pub mod epg;
pub mod favorites;
pub mod pairing;
pub mod recents;
pub mod search;
pub mod settings;
pub mod source;

pub use catalog::CatalogService;
pub use custom::CustomChannelService;
pub use epg::{
    EpgRefreshListener, EpgRefreshOutcome, EpgRefreshProgress, EpgRefreshStage, EpgService,
};
pub use favorites::FavoritesService;
pub use pairing::{PairingListener, PairingService, PairingSession, PairingSubmission};
pub use recents::RecentsService;
pub use search::SearchService;
pub use settings::SettingsService;
pub use source::SourceService;
