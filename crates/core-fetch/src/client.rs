// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! reqwest + rustls client construction with timeouts and a redirect hop cap
//! (TECH_SPEC §4.5).
//!
//! All HTTP in Spidola flows through here. The client is configured once per source
//! (timeouts, redirect cap, per-source TLS posture and default user-agent) and reused; the
//! streaming body path lives in [`crate::body`]. rustls is the only TLS backend — no
//! OpenSSL in the dependency tree.

use std::time::Duration;

use reqwest::redirect::Policy;

use crate::error::{FetchError, FetchResult, classify};
use crate::headers::{self, RequestSpec};
use crate::tls;

/// Transport configuration for a source's client.
#[derive(Debug, Clone)]
pub struct FetchConfig {
    /// Maximum time to establish a connection.
    pub connect_timeout: Duration,
    /// Overall per-request deadline (import bytes must arrive within this).
    pub request_timeout: Duration,
    /// Maximum redirect hops before failing.
    pub max_redirects: usize,
    /// Per-source self-signed-TLS escape hatch (off by default, [`crate::tls`]).
    pub accept_invalid_tls: bool,
    /// Default user-agent when a request does not override it.
    pub default_user_agent: String,
}

impl Default for FetchConfig {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(10),
            request_timeout: Duration::from_mins(1),
            max_redirects: 5,
            accept_invalid_tls: false,
            default_user_agent: concat!("Spidola/", env!("CARGO_PKG_VERSION")).to_owned(),
        }
    }
}

/// A configured HTTP client for one source.
#[derive(Debug, Clone)]
pub struct HttpClient {
    inner: reqwest::Client,
}

impl HttpClient {
    /// Builds a client from `config`.
    ///
    /// # Errors
    /// Returns [`FetchError::Build`] if the TLS backend or client cannot be constructed.
    pub fn new(config: &FetchConfig) -> FetchResult<Self> {
        let builder = reqwest::Client::builder()
            .connect_timeout(config.connect_timeout)
            .timeout(config.request_timeout)
            .redirect(Policy::limited(config.max_redirects))
            .user_agent(config.default_user_agent.clone());
        let builder = tls::apply(builder, config.accept_invalid_tls);
        let inner = builder.build().map_err(FetchError::Build)?;
        Ok(Self { inner })
    }

    /// Issues a GET and returns the response once headers arrive, mapping a non-success
    /// status into [`FetchError::Status`]. The body is consumed via [`crate::body`].
    ///
    /// # Errors
    /// Returns a classified [`FetchError`] on transport failure or a non-2xx status.
    pub async fn get(&self, spec: &RequestSpec<'_>) -> FetchResult<reqwest::Response> {
        let builder = headers::apply(self.inner.get(spec.url), spec)?;
        let response = builder.send().await.map_err(classify)?;
        let status = response.status();
        if status.is_success() {
            Ok(response)
        } else {
            Err(FetchError::Status {
                status: status.as_u16(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn default_config_disables_the_tls_hatch() {
        let cfg = FetchConfig::default();
        assert!(
            !cfg.accept_invalid_tls,
            "the self-signed escape hatch must default off"
        );
        assert_eq!(cfg.max_redirects, 5);
    }

    #[test]
    fn clients_build_independently_for_each_tls_posture() {
        // The hatch is per-client: building one with it on must not affect a strict one.
        let strict = HttpClient::new(&FetchConfig::default());
        let lax = HttpClient::new(&FetchConfig {
            accept_invalid_tls: true,
            ..FetchConfig::default()
        });
        assert!(strict.is_ok());
        assert!(lax.is_ok());
    }
}
