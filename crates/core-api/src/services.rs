// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! `services` — one file per service (paged-by-contract read paths).
//!
//! Phase 0 skeleton: modules are declared per TECH_SPEC §3.2; their public
//! surface is re-exported from here as it lands in later phases.
#![forbid(unsafe_code)]

pub mod catalog;
pub mod epg;
pub mod favorites;
pub mod pairing;
pub mod search;
pub mod settings;
pub mod source;
