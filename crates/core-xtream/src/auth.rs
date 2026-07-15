// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Xtream authentication handshake (TECH_SPEC §4.3).
//!
//! `player_api.php` with no action is both the login and the account-status call: it either
//! answers with a `user_info` block describing the subscription, or it answers with one
//! saying no. [`authenticate`] turns that into either an [`AccountStatus`] or a typed
//! [`XtreamError::Unauthorized`], so no caller has to know how a given panel spells refusal.
//!
//! The password reaches this module as a borrowed `&Secret` from the host-secrets callback
//! and is passed straight through to `crate::urls`; nothing here stores or renders it (§12).

use core_fetch::HttpClient;
use core_model::secret::Secret;

use crate::LOG_TARGET;
use crate::error::{AuthRejection, XtreamError, XtreamResult};
use crate::request;
use crate::urls::Endpoint;
use crate::wire::{self, AuthEnvelope, UserInfo};

/// What a headend says about an account once it has accepted it.
///
/// Not the wire's `user_info` — that block also mirrors the credentials back, carries a
/// dozen fields no screen shows, and spells its numbers three ways. This is the part the
/// product actually has a use for, in domain terms (times are Unix seconds, per
/// `core-model`'s convention).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountStatus {
    /// When the subscription lapses, if the headend states an expiry. `None` means either
    /// "never" or "not said" — the two are indistinguishable on the wire, and both mean
    /// the UI should not render a countdown.
    pub expires_at: Option<i64>,
    /// How many concurrent streams the account allows, if stated.
    pub max_connections: Option<u64>,
    /// How many of those are in use right now, if stated.
    pub active_connections: Option<u64>,
}

/// Performs the handshake, returning the account's status or the reason it was refused.
///
/// # Errors
/// Returns [`XtreamError::Unauthorized`] when the headend rejects the account (see
/// [`AuthRejection`] for the flavours), [`XtreamError::Malformed`] if the response is not a
/// handshake at all, or [`XtreamError::Transport`] if the request never landed.
pub async fn authenticate(
    http: &HttpClient,
    endpoint: &Endpoint,
    password: &Secret,
) -> XtreamResult<AccountStatus> {
    let body = request::get(http, endpoint, password, &[]).await?;
    let envelope: AuthEnvelope = wire::parse_object(&body)?;
    let info = envelope.user_info.ok_or_else(|| XtreamError::Malformed {
        detail: "the handshake response carried no account block".to_owned(),
    })?;
    interpret(&info)
}

/// Decides whether `info` describes a usable account.
///
/// Split from the request so every rejection rule is unit-testable against a fixture
/// without a server in the loop.
fn interpret(info: &UserInfo) -> XtreamResult<AccountStatus> {
    // `auth` is the headend's yes/no. Absent means "didn't say", which older panels do on
    // success; only an explicit no is a refusal, and the status check below still applies.
    if info.auth == Some(false) {
        return Err(reject(AuthRejection::Credentials));
    }
    if let Some(status) = info.status.as_deref() {
        let rejection = match status.trim().to_ascii_lowercase().as_str() {
            "active" => None,
            "expired" => Some(AuthRejection::Expired),
            "banned" | "disabled" => Some(AuthRejection::Banned),
            // A status we don't recognize is not a status we can call healthy. Panels that
            // mean well say `Active`; the rest get the honest catch-all.
            _ => Some(AuthRejection::Inactive),
        };
        if let Some(rejection) = rejection {
            return Err(reject(rejection));
        }
    }
    Ok(AccountStatus {
        expires_at: info.exp_date.filter(|secs| *secs > 0),
        max_connections: info.max_connections,
        active_connections: info.active_cons,
    })
}

/// Builds the refusal and logs it. The account is never named in the log line — the
/// rejection label is the whole of what a support thread needs (§4.8).
fn reject(rejection: AuthRejection) -> XtreamError {
    tracing::warn!(
        target: LOG_TARGET,
        rejection = rejection.as_str(),
        "the headend rejected the account"
    );
    XtreamError::Unauthorized { rejection }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    /// Interprets a `user_info` block spelled as a real headend would spell it.
    fn interpret_json(json: &str) -> XtreamResult<AccountStatus> {
        let envelope: AuthEnvelope = serde_json::from_str(json).unwrap();
        interpret(&envelope.user_info.expect("fixture must carry user_info"))
    }

    fn rejection_of(json: &str) -> AuthRejection {
        match interpret_json(json) {
            Err(XtreamError::Unauthorized { rejection }) => rejection,
            other => panic!("expected a rejection, got {other:?}"),
        }
    }

    #[test]
    fn an_active_account_yields_its_status() {
        let status = interpret_json(
            r#"{"user_info": {
                "auth": 1, "status": "Active",
                "exp_date": "1735689600", "max_connections": "2", "active_cons": "1"
            }}"#,
        )
        .unwrap();
        assert_eq!(
            status,
            AccountStatus {
                expires_at: Some(1_735_689_600),
                max_connections: Some(2),
                active_connections: Some(1),
            }
        );
    }

    #[test]
    fn auth_zero_is_a_credential_rejection_whatever_the_status_says() {
        assert_eq!(
            rejection_of(r#"{"user_info": {"auth": 0}}"#),
            AuthRejection::Credentials
        );
        assert_eq!(
            rejection_of(r#"{"user_info": {"auth": "0", "status": "Active"}}"#),
            AuthRejection::Credentials,
            "auth is the headend's yes/no; a stale `Active` must not override it"
        );
    }

    #[test]
    fn each_unhealthy_status_maps_to_its_own_rejection() {
        assert_eq!(
            rejection_of(r#"{"user_info": {"auth": 1, "status": "Expired"}}"#),
            AuthRejection::Expired
        );
        assert_eq!(
            rejection_of(r#"{"user_info": {"auth": 1, "status": "Banned"}}"#),
            AuthRejection::Banned
        );
        assert_eq!(
            rejection_of(r#"{"user_info": {"auth": 1, "status": "Disabled"}}"#),
            AuthRejection::Banned
        );
        // A status nobody has seen before is not assumed healthy.
        assert_eq!(
            rejection_of(r#"{"user_info": {"auth": 1, "status": "Kicked"}}"#),
            AuthRejection::Inactive
        );
    }

    #[test]
    fn status_matching_ignores_case_and_padding() {
        assert!(interpret_json(r#"{"user_info": {"auth": 1, "status": " active "}}"#).is_ok());
        assert!(interpret_json(r#"{"user_info": {"auth": 1, "status": "ACTIVE"}}"#).is_ok());
    }

    #[test]
    fn a_silent_but_unrefused_account_is_accepted() {
        // Older panels answer the handshake with neither `auth` nor `status`. Refusing them
        // would lock out working accounts, so silence is not refusal.
        let status = interpret_json(r#"{"user_info": {}}"#).unwrap();
        assert_eq!(status.expires_at, None);
        assert_eq!(status.max_connections, None);
    }

    #[test]
    fn a_placeholder_expiry_is_reported_as_no_expiry() {
        // Headends spell "unlimited" as null, "", or 0; none of them is a real epoch date.
        for json in [
            r#"{"user_info": {"auth": 1, "exp_date": null}}"#,
            r#"{"user_info": {"auth": 1, "exp_date": ""}}"#,
            r#"{"user_info": {"auth": 1, "exp_date": 0}}"#,
            r#"{"user_info": {"auth": 1, "exp_date": "0"}}"#,
        ] {
            assert_eq!(
                interpret_json(json).unwrap().expires_at,
                None,
                "{json} should not read as an expiry"
            );
        }
    }
}
