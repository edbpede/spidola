// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! `core-model` — domain types; illegal states must not construct (parse, don't validate).
//!
//! Phase 0 skeleton: modules are declared per TECH_SPEC §3.2; their public
//! surface is re-exported from here as it lands in later phases.
#![forbid(unsafe_code)]

pub mod category;
pub mod channel;
pub mod epg;
pub mod favorite;
pub mod history;
pub mod ids;
pub mod locator;
pub mod secret;
pub mod source;
