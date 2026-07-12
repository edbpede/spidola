// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Per-source self-signed-TLS escape hatch (TECH_SPEC §4.5, §12).
//!
//! Self-hosted headends with self-signed certificates are common, so an opt-in
//! "accept invalid TLS" hatch exists — but it is **off by default**, **loudly labeled**
//! (the source list carries a persistent badge, §12), and **scoped to a single source**:
//! it is applied to that source's own [`reqwest::Client`], never globally, so one source's
//! choice cannot weaken TLS for any other. TLS itself is rustls with platform roots; no
//! OpenSSL anywhere in the tree.

use reqwest::ClientBuilder;

/// Applies the per-source TLS posture to a client builder.
///
/// With `accept_invalid_tls == false` (the default) the builder is returned unchanged, so
/// certificates are verified normally. With `true`, certificate validation is disabled for
/// **this client only**.
pub(crate) fn apply(builder: ClientBuilder, accept_invalid_tls: bool) -> ClientBuilder {
    if accept_invalid_tls {
        builder.danger_accept_invalid_certs(true)
    } else {
        builder
    }
}
