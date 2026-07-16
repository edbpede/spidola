// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Per-source user-agent and header injection (TECH_SPEC §4.5).
//!
//! A [`RequestSpec`] carries the URL plus the per-source user-agent and extra headers. Any
//! token-bearing header value must be sourced through the host-secrets callback by the
//! caller; this layer only applies what it is given and never logs header values (§12).

use reqwest::RequestBuilder;
use reqwest::header::{HeaderName, HeaderValue};

use crate::error::FetchError;

/// Everything needed to shape a single request beyond the HTTP method.
#[derive(Debug, Clone, Copy)]
pub struct RequestSpec<'a> {
    /// Absolute request URL.
    pub url: &'a str,
    /// Optional per-source user-agent override (falls back to the client default).
    pub user_agent: Option<&'a str>,
    /// Extra `(name, value)` headers to inject.
    pub headers: &'a [(String, String)],
}

impl<'a> RequestSpec<'a> {
    /// A bare GET spec with no overrides.
    #[must_use]
    pub fn new(url: &'a str) -> Self {
        Self {
            url,
            user_agent: None,
            headers: &[],
        }
    }
}

/// Validates request overrides without constructing or sending a request.
///
/// # Errors
/// Returns [`FetchError::InvalidHeader`] when the user agent or any header is not valid HTTP.
pub fn validate(user_agent: Option<&str>, headers: &[(String, String)]) -> Result<(), FetchError> {
    if let Some(agent) = user_agent {
        HeaderValue::from_str(agent).map_err(|error| FetchError::InvalidHeader {
            name: "User-Agent".to_owned(),
            reason: error.to_string(),
        })?;
    }
    for (name, value) in headers {
        name.parse::<HeaderName>()
            .map_err(|error| FetchError::InvalidHeader {
                name: name.clone(),
                reason: error.to_string(),
            })?;
        HeaderValue::from_str(value).map_err(|error| FetchError::InvalidHeader {
            name: name.clone(),
            reason: error.to_string(),
        })?;
    }
    Ok(())
}

/// Applies the spec's user-agent and headers to a request builder.
///
/// # Errors
/// Returns [`FetchError::InvalidHeader`] if a header name or value is not valid HTTP.
pub(crate) fn apply(
    mut builder: RequestBuilder,
    spec: &RequestSpec<'_>,
) -> Result<RequestBuilder, FetchError> {
    validate(spec.user_agent, spec.headers)?;
    if let Some(ua) = spec.user_agent {
        let value = HeaderValue::from_str(ua).map_err(|e| FetchError::InvalidHeader {
            name: "User-Agent".to_owned(),
            reason: e.to_string(),
        })?;
        builder = builder.header(reqwest::header::USER_AGENT, value);
    }
    for (name, value) in spec.headers {
        let header_name = name
            .parse::<HeaderName>()
            .map_err(|e| FetchError::InvalidHeader {
                name: name.clone(),
                reason: e.to_string(),
            })?;
        let header_value = HeaderValue::from_str(value).map_err(|e| FetchError::InvalidHeader {
            name: name.clone(),
            // The reason never echoes the value — only the parser's classification.
            reason: e.to_string(),
        })?;
        builder = builder.header(header_name, header_value);
    }
    Ok(builder)
}
