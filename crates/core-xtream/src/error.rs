// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! `core-xtream`'s error taxonomy (TECH_SPEC §4.7, standing rule).
//!
//! The split mirrors `core-parse`'s tolerance posture (§4.2, §4.3): a malformed *entry* is
//! never an error — it is skipped and counted in [`crate::diagnostics::Diagnostics`] so one
//! bad row cannot fail an import — while a malformed *envelope* (the response is not the
//! shape the endpoint contractually returns) is a typed [`XtreamError::Malformed`]. Nothing
//! here carries credential material: [`XtreamError::Malformed`]'s detail is the
//! deserializer's structural complaint, which never echoes the payload.

use core_fetch::FetchError;
use thiserror::Error;

/// Result alias for the Xtream layer.
pub type XtreamResult<T> = Result<T, XtreamError>;

/// Why a headend refused the account.
///
/// Modelled as an enum rather than the server's raw status string so the shells can attach
/// the PRD's prescribed action to each case (an error with no action is a design bug,
/// §4.7); `core-api` flattens the whole set onto its single FFI `Unauthorized` variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthRejection {
    /// `user_info.auth` was `0` — the username/password pair was rejected outright.
    Credentials,
    /// `user_info.status` was `Expired` — the subscription lapsed and can be renewed.
    Expired,
    /// `user_info.status` was `Banned` — the headend blocked this account.
    Banned,
    /// `user_info.status` was neither `Active` nor a status we recognize. The account exists
    /// but is not usable; the honest catch-all rather than guessing at a vendor's wording.
    Inactive,
}

impl AuthRejection {
    /// A stable, non-identifying label for the log stream.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Credentials => "credentials",
            Self::Expired => "expired",
            Self::Banned => "banned",
            Self::Inactive => "inactive",
        }
    }
}

impl std::fmt::Display for AuthRejection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// An Xtream-layer failure.
#[derive(Debug, Error)]
pub enum XtreamError {
    /// The request never produced a usable response (DNS, TLS, timeout, non-2xx status).
    #[error("the Xtream request failed")]
    Transport(#[from] FetchError),

    /// The headend answered, and the answer was "no".
    #[error("the headend rejected the account ({rejection})")]
    Unauthorized {
        /// Which flavour of "no", so the UI can name the fix.
        rejection: AuthRejection,
    },

    /// The response was not the shape the endpoint contractually returns (not JSON, an
    /// object where an array was promised, a missing `user_info`, …).
    ///
    /// This is an *envelope* failure. A single unusable row inside a well-formed array is
    /// not this — it is a counted skip (`crate::diagnostics`).
    #[error("the headend returned a malformed response: {detail}")]
    Malformed {
        /// The structural complaint. Never echoes the response payload, so a response body
        /// that embeds credentials (Xtream's `user_info` mirrors the password back) cannot
        /// reach the log stream through here.
        detail: String,
    },

    /// The response body exceeded [`crate::request::MAX_BODY_BYTES`].
    ///
    /// Xtream catalogs are returned as one JSON array with no pagination, so the body must
    /// be buffered whole; the cap stops a broken or hostile headend from exhausting memory
    /// on a 1 GB device (the bounded-memory posture of §4.2).
    #[error("the headend's response exceeded the {limit}-byte cap")]
    ResponseTooLarge {
        /// The cap that was exceeded, in bytes.
        limit: usize,
    },

    /// The account's server URL cannot host an Xtream API (it is not a hierarchical URL, so
    /// no path can be appended to it).
    #[error("the server address cannot be used as an Xtream base URL: {reason}")]
    InvalidServer {
        /// Why the address was refused. Contains only the URL's structural problem.
        reason: String,
    },
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn rejections_render_as_stable_labels() {
        assert_eq!(AuthRejection::Credentials.to_string(), "credentials");
        assert_eq!(AuthRejection::Expired.to_string(), "expired");
        assert_eq!(AuthRejection::Banned.to_string(), "banned");
        assert_eq!(AuthRejection::Inactive.to_string(), "inactive");
    }

    #[test]
    fn unauthorized_names_the_rejection_in_its_message() {
        let err = XtreamError::Unauthorized {
            rejection: AuthRejection::Expired,
        };
        assert_eq!(
            err.to_string(),
            "the headend rejected the account (expired)"
        );
    }

    #[test]
    fn transport_preserves_its_source_chain() {
        // `#[from]` must wire `source()` through so the log stream keeps the full chain
        // while the UI only ever sees the top-level message (§4.7).
        let err = XtreamError::from(FetchError::Status { status: 502 });
        assert_eq!(err.to_string(), "the Xtream request failed");
        let source = std::error::Error::source(&err).expect("transport must carry a source");
        assert_eq!(source.to_string(), "source returned HTTP 502");
    }
}
