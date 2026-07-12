// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! `core-fetch` — all HTTP lives here (reqwest + rustls; no OpenSSL, TECH_SPEC §4.5).
//!
//! Phase 0 skeleton: modules are declared per TECH_SPEC §3.2; their public
//! surface is re-exported from here as it lands in later phases.
#![forbid(unsafe_code)]

pub mod body;
pub mod client;
pub mod headers;
pub mod tls;
