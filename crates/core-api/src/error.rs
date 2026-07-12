// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The flattened, stable FFI error taxonomy with its variant → UX mapping (PRD §6.3,
//! TECH_SPEC §4.7).
//!
//! `core-api` flattens each crate's internal error enum into this small, stable set that
//! crosses the FFI. Every variant carries only the minimal structured data the UI needs;
//! the full diagnostic chain is preserved into the **log stream**, never the FFI error, so
//! user-facing messages stay clean (no system jargon — PRD §8.6) while diagnostics stay
//! rich. Every variant maps to a plain-language failure class with at least one prescribed
//! action: an error with no action is a design bug, made unrepresentable by the
//! `every_variant_has_an_action` test below.
//!
//! Note: the playback "Try other player" loud-fallback is driven by the shells' own
//! `EngineError` (TECH_SPEC §8, Phase 5), not by this core-side taxonomy — these variants
//! cover source ingestion, catalog, search, and storage.

use thiserror::Error;

use core_db::DbError;
use core_fetch::FetchError;
use core_model::ModelError;
use core_search::SearchError;

/// The stable, user-mappable error surface the shells receive across the FFI.
#[derive(Debug, Clone, PartialEq, Eq, Error, uniffi::Error)]
pub enum ApiError {
    /// The source's server could not be reached.
    #[error("can't reach the source right now")]
    NetworkUnreachable,

    /// A request took too long.
    #[error("the source is taking too long to respond")]
    Timeout,

    /// The source rejected the supplied credentials.
    #[error("the source didn't accept your login")]
    Unauthorized,

    /// The requested item no longer exists at the source.
    #[error("that isn't available at the source anymore")]
    NotFound,

    /// Input provided by the user was not usable.
    #[error("that entry isn't valid: {reason}")]
    InvalidInput {
        /// A short, plain-language reason (never contains secret material).
        reason: String,
    },

    /// The response was fetched but yielded no usable channels.
    #[error("this source didn't contain any channels")]
    ParseFailed {
        /// Channels successfully read.
        emitted: u64,
        /// Entries skipped as malformed.
        skipped: u64,
    },

    /// The local database could not be read or written.
    #[error("something went wrong with local storage")]
    StorageCorrupt,

    /// The operation was cancelled (by the user or a departing screen).
    #[error("that was cancelled")]
    Cancelled,

    /// An unexpected internal error; the detail is in the log stream.
    #[error("something unexpected went wrong")]
    Internal,
}

/// A prescribed user action for an error's UX (PRD §6.3). The set is deliberately small;
/// the shell renders each as a button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserAction {
    /// Try the same operation again.
    Retry,
    /// Return to the previous screen.
    GoBack,
    /// Correct the input that caused the failure.
    FixInput,
}

/// The plain-language presentation of an error: a failure class and its prescribed actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ErrorUx {
    /// A short, couch-legible failure class (PRD §8.6 voice).
    pub failure_class: &'static str,
    /// The actions offered; always non-empty.
    pub actions: &'static [UserAction],
}

impl ApiError {
    /// Maps this error to its plain-language failure class and prescribed actions.
    ///
    /// This table is the single source of truth cross-checked against PRD §6.3; the shells'
    /// error-presentation component (Phase 4) renders from it.
    #[must_use]
    pub fn ux(&self) -> ErrorUx {
        use UserAction::{FixInput, GoBack, Retry};
        match self {
            Self::NetworkUnreachable => ErrorUx {
                failure_class: "Can't reach the source",
                actions: &[Retry, GoBack],
            },
            Self::Timeout => ErrorUx {
                failure_class: "The source is slow to respond",
                actions: &[Retry, GoBack],
            },
            Self::Unauthorized => ErrorUx {
                failure_class: "Login was rejected",
                actions: &[FixInput, GoBack],
            },
            Self::NotFound => ErrorUx {
                failure_class: "Not available anymore",
                actions: &[GoBack],
            },
            Self::InvalidInput { .. } => ErrorUx {
                failure_class: "That entry isn't valid",
                actions: &[FixInput, GoBack],
            },
            Self::ParseFailed { .. } => ErrorUx {
                failure_class: "No channels found",
                actions: &[Retry, GoBack],
            },
            Self::StorageCorrupt => ErrorUx {
                failure_class: "Local storage problem",
                actions: &[Retry, GoBack],
            },
            Self::Cancelled => ErrorUx {
                failure_class: "Cancelled",
                actions: &[GoBack],
            },
            Self::Internal => ErrorUx {
                failure_class: "Something went wrong",
                actions: &[Retry, GoBack],
            },
        }
    }
}

impl From<FetchError> for ApiError {
    fn from(error: FetchError) -> Self {
        match error {
            FetchError::Timeout(_) => Self::Timeout,
            FetchError::Status { status } => match status {
                401 | 403 => Self::Unauthorized,
                404 | 410 => Self::NotFound,
                _ => Self::NetworkUnreachable,
            },
            FetchError::InvalidHeader { .. } => Self::InvalidInput {
                reason: "a request header wasn't valid".to_owned(),
            },
            FetchError::Connect(_)
            | FetchError::TooManyRedirects(_)
            | FetchError::Build(_)
            | FetchError::Body(_)
            | FetchError::Request(_) => Self::NetworkUnreachable,
        }
    }
}

impl From<DbError> for ApiError {
    fn from(_error: DbError) -> Self {
        // Every persistence failure presents as a storage problem; the specific cause
        // (query/migration/serde) goes to the log stream, not the FFI error.
        Self::StorageCorrupt
    }
}

impl From<SearchError> for ApiError {
    fn from(_error: SearchError) -> Self {
        Self::StorageCorrupt
    }
}

impl From<ModelError> for ApiError {
    fn from(error: ModelError) -> Self {
        Self::InvalidInput {
            reason: match error {
                ModelError::InvalidLocator { .. } => "that isn't a valid stream address".to_owned(),
                ModelError::Empty { field } => format!("{field} can't be empty"),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    /// Every variant, exhaustively, so adding one forces a UX decision here.
    fn all_variants() -> Vec<ApiError> {
        vec![
            ApiError::NetworkUnreachable,
            ApiError::Timeout,
            ApiError::Unauthorized,
            ApiError::NotFound,
            ApiError::InvalidInput {
                reason: "x".to_owned(),
            },
            ApiError::ParseFailed {
                emitted: 0,
                skipped: 3,
            },
            ApiError::StorageCorrupt,
            ApiError::Cancelled,
            ApiError::Internal,
        ]
    }

    #[test]
    fn every_variant_has_an_action() {
        for error in all_variants() {
            assert!(
                !error.ux().actions.is_empty(),
                "{error:?} has no prescribed action — that is a design bug (PRD §6.3)"
            );
            assert!(!error.ux().failure_class.is_empty());
        }
    }

    #[test]
    fn user_messages_are_jargon_free() {
        // No system jargon reaches the screen (PRD §8.6): the Display strings must not
        // mention parsers, FFI, SQL, or HTTP internals.
        let banned = [
            "FFI", "SQL", "SQLite", "HTTP", "parse", "rusqlite", "reqwest",
        ];
        for error in all_variants() {
            let message = error.to_string().to_lowercase();
            for term in banned {
                assert!(
                    !message.contains(&term.to_lowercase()),
                    "user message `{message}` leaks jargon `{term}`"
                );
            }
        }
    }

    #[test]
    fn http_status_maps_to_the_right_class() {
        assert_eq!(
            ApiError::from(FetchError::Status { status: 401 }),
            ApiError::Unauthorized
        );
        assert_eq!(
            ApiError::from(FetchError::Status { status: 404 }),
            ApiError::NotFound
        );
        assert_eq!(
            ApiError::from(FetchError::Status { status: 500 }),
            ApiError::NetworkUnreachable
        );
    }

    #[test]
    fn model_errors_become_actionable_input_errors() {
        let err = ApiError::from(ModelError::Empty { field: "name" });
        assert!(matches!(err, ApiError::InvalidInput { .. }));
        assert_eq!(
            err.ux().actions,
            &[UserAction::FixInput, UserAction::GoBack]
        );
    }
}
