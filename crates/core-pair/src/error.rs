// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! `core-pair`'s error taxonomy (TECH_SPEC §4.7, standing rule).
//!
//! Two error domains, deliberately separate because they have different audiences.
//! [`PairError`] is what the *shell* sees: the server could not come up, so the pairing
//! screen must say so instead of pretending. `Rejection` (crate-private) is what a *stranger
//! on the wire* sees: the reason a request is refused, collapsed to an HTTP status. Keeping
//! them apart is what stops a hostile request from ever looking like a server fault to the
//! UI — and keeping `Rejection` private is what stops the shell from being handed a
//! vocabulary it has no use for.

use thiserror::Error;

/// The pairing server could not start. Every variant is terminal for one `start` attempt;
/// the screen surfaces it as "pairing is unavailable" rather than retrying blindly.
#[derive(Debug, Error)]
pub enum PairError {
    /// The listening socket could not be bound. Effectively only sandbox/permission
    /// failures, since the port is ephemeral and so cannot already be in use.
    #[error("could not bind the pairing server")]
    Bind(#[source] std::io::Error),

    /// No route out of this host, so there was nothing to infer an address from. Practically:
    /// the TV is not attached to a network at all.
    #[error("could not determine a private IPv4 address for this host")]
    NoLanAddress(#[source] std::io::Error),

    /// The address to advertise is not one a phone on the LAN could dial — not private
    /// (RFC 1918), link-local, or loopback. Raised by `PairServer::start` when the route out
    /// of this host starts from such an address, and by `PairServer::start_advertising` when
    /// a caller supplies one.
    ///
    /// **Read this variant literally, and render it carefully.** It states one fact: *this
    /// address* is not dialable from the LAN. It does **not** establish that the TV is off
    /// the network. A full-tunnel VPN produces it on a perfectly connected TV, because the
    /// route out belongs to the tunnel (measured: Wi-Fi `192.168.50.98`, VPN `100.80.166.175`
    /// — the probe returns the tunnel). A shell that renders this as "connect your TV to your
    /// network" will be lying to a user whose TV is already connected; the honest reading is
    /// "we could not work out your TV's address", and the fix is
    /// `PairServer::start_advertising` with the address the platform reports.
    #[error("{address} is not an address a phone on the local network could reach")]
    NotOnPrivateNetwork {
        /// The address that was rejected. Carried so diagnostics name it — a support thread
        /// seeing `100.80.166.175` learns "VPN" immediately, where a bare message would not.
        address: std::net::IpAddr,
    },
}

/// Why one HTTP request was refused, and with what status.
///
/// Every variant is a *refusal*, never a server fault: the whole point of the hardening
/// budget is that hostile input is ordinary, expected traffic that costs a 4xx and nothing
/// else. Variants carry no request bytes, so a rejection can be logged whole (§4.8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub(crate) enum Rejection {
    /// The peer's IP is not on a private network, link-local, or loopback. The teeth behind
    /// "a person on the network cannot inject a source into a TV they cannot see" (§12).
    #[error("peer is not on the local network")]
    NotLocal,

    /// The submitted token is absent or does not match this session's.
    ///
    /// Deliberately one variant, not "missing" plus "wrong": the response must not tell a
    /// guesser which of the two happened, nor how close they were.
    #[error("token rejected")]
    BadToken,

    /// The client took longer than the read budget to deliver a complete request
    /// (slow-loris, or simply a dead connection).
    #[error("the request timed out")]
    Timeout,

    /// The request line exceeded its byte cap before a newline arrived.
    #[error("the request line is too long")]
    RequestLineTooLong,

    /// The header block exceeded its count or total-byte cap.
    #[error("the request headers are too large")]
    HeadersTooLarge,

    /// The declared or delivered body exceeded its byte cap.
    #[error("the request body is too large")]
    BodyTooLarge,

    /// The request was not intelligible HTTP/1.1, or the form body was not intelligible
    /// `application/x-www-form-urlencoded`.
    #[error("the request is malformed")]
    Malformed,

    /// The method/path pair is not one of the two routes that exist.
    #[error("no such route")]
    NoRoute,
}

impl Rejection {
    /// The HTTP status this refusal is served as.
    pub(crate) fn status(self) -> Status {
        match self {
            Self::NotLocal | Self::BadToken => Status::FORBIDDEN,
            Self::Timeout => Status::REQUEST_TIMEOUT,
            Self::RequestLineTooLong => Status::URI_TOO_LONG,
            Self::HeadersTooLarge => Status::HEADERS_TOO_LARGE,
            Self::BodyTooLarge => Status::PAYLOAD_TOO_LARGE,
            Self::Malformed => Status::BAD_REQUEST,
            Self::NoRoute => Status::NOT_FOUND,
        }
    }
}

/// An HTTP status code paired with its reason phrase.
///
/// A three-field struct rather than a bare `u16` so the status line cannot be assembled
/// with a mismatched phrase, and so the set of statuses this server can emit is legible in
/// one place. Only the codes below are ever served.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Status {
    /// The numeric status code.
    pub(crate) code: u16,
    /// The reason phrase for the status line.
    pub(crate) reason: &'static str,
}

impl Status {
    /// The form was accepted and handed to the sink.
    pub(crate) const OK: Self = Self {
        code: 200,
        reason: "OK",
    };
    /// The request or its form body did not parse.
    pub(crate) const BAD_REQUEST: Self = Self {
        code: 400,
        reason: "Bad Request",
    };
    /// The peer is off-LAN, or the token did not match.
    pub(crate) const FORBIDDEN: Self = Self {
        code: 403,
        reason: "Forbidden",
    };
    /// Neither `GET /` nor `POST /`.
    pub(crate) const NOT_FOUND: Self = Self {
        code: 404,
        reason: "Not Found",
    };
    /// The read budget elapsed before a complete request arrived.
    pub(crate) const REQUEST_TIMEOUT: Self = Self {
        code: 408,
        reason: "Request Timeout",
    };
    /// The body exceeded its cap.
    pub(crate) const PAYLOAD_TOO_LARGE: Self = Self {
        code: 413,
        reason: "Content Too Large",
    };
    /// The request line exceeded its cap.
    pub(crate) const URI_TOO_LONG: Self = Self {
        code: 414,
        reason: "URI Too Long",
    };
    /// The header block exceeded its count or byte cap.
    pub(crate) const HEADERS_TOO_LARGE: Self = Self {
        code: 431,
        reason: "Request Header Fields Too Large",
    };
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn every_rejection_maps_to_a_client_error_status() {
        let all = [
            Rejection::NotLocal,
            Rejection::BadToken,
            Rejection::Timeout,
            Rejection::RequestLineTooLong,
            Rejection::HeadersTooLarge,
            Rejection::BodyTooLarge,
            Rejection::Malformed,
            Rejection::NoRoute,
        ];
        for rejection in all {
            let status = rejection.status();
            assert!(
                (400..500).contains(&status.code),
                "{rejection:?} must be a client error, got {}",
                status.code
            );
        }
    }

    #[test]
    fn a_bad_token_is_indistinguishable_from_a_missing_one() {
        // One variant, one status, one rendered string — there is no second shape for a
        // guesser to tell "missing" and "wrong" apart by.
        assert_eq!(Rejection::BadToken.status(), Status::FORBIDDEN);
        assert_eq!(Rejection::BadToken.to_string(), "token rejected");
    }
}
