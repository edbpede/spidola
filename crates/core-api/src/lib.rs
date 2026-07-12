// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! `core-api` — the façade composing the other crates and the composition root.
//!
//! Phase 1 lands the two load-bearing scaffolds that later phases build on: the flattened,
//! stable FFI [`error`] taxonomy (with its variant → UX mapping, PRD §6.3) and the
//! [`logging`] pipeline (`tracing` + a ring-buffer export, TECH_SPEC §4.8). The owned Tokio
//! runtime, UniFFI surface, and services land in Phase 2.
//!
//! Note: unlike the other crates this one does NOT `#![forbid(unsafe_code)]`, because it
//! will host UniFFI-generated FFI glue; `unsafe` is warned at the workspace level.

pub mod error;
pub mod events;
pub mod logging;
pub mod runtime;
pub mod secrets;
pub mod services;

pub use error::{ApiError, ErrorUx, UserAction};
pub use logging::{LogConfig, LogHandle, RingBuffer, RingLayer};
