// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! `repo` — one repository per aggregate.
//!
//! Phase 0 skeleton: modules are declared per TECH_SPEC §3.2; their public
//! surface is re-exported from here as it lands in later phases.
#![forbid(unsafe_code)]

pub mod channels;
pub mod custom;
pub mod epg;
pub mod favorites;
pub mod history;
pub mod settings;
pub mod sources;
