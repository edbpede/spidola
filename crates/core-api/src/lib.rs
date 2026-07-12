// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! `core-api` — the façade composing the other crates and the composition root.
//!
//! Phase 0 skeleton: modules are declared per TECH_SPEC §3.2. UniFFI scaffolding,
//! the owned Tokio runtime, and the service surface land in Phase 2.
//!
//! Note: unlike the other crates this one does NOT `#![forbid(unsafe_code)]`, because
//! it will host UniFFI-generated FFI glue; `unsafe` is warned at the workspace level.

pub mod error;
pub mod events;
pub mod logging;
pub mod runtime;
pub mod secrets;
pub mod services;
