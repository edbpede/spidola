// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! `xmltv` — the streaming XMLTV parser façade (rolling-window filtered during parse).
//!
//! Phase 0 skeleton: modules are declared per TECH_SPEC §3.2; their public
//! surface is re-exported from here as it lands in later phases.
#![forbid(unsafe_code)]

pub mod pull;
pub mod window;
