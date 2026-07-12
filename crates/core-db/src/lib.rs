// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! `core-db` — SQLite persistence; every entry point is a blocking function (TECH_SPEC §4.4).
//!
//! Phase 0 skeleton: modules are declared per TECH_SPEC §3.2; their public
//! surface is re-exported from here as it lands in later phases.
#![forbid(unsafe_code)]

pub mod migrations;
pub mod pool;
pub mod refresh;
pub mod repo;
pub mod search_index;
