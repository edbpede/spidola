// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! `core-pair` — the LAN pairing micro-server; alive only while its screen is visible (TECH_SPEC §12).
//!
//! Typing a playlist URL on a TV remote is miserable, so PRD §6.1 offers a shortcut: the TV
//! shows a local address and a code, the user opens the address on their phone, and pastes
//! the details into a plain form. This crate is that form and the server behind it — and
//! nothing else. It holds no user data, reads no database, and forgets everything when the
//! screen closes.
//!
//! The shell provides the pixels and receives the result. It renders [`PairServer::url`] and
//! [`PairServer::token`] on the pairing screen (as text and as a QR code — the QR is the
//! shell's business, not this crate's), and implements [`SubmissionSink`] to receive what the
//! phone sends as a pre-filled add-source flow. Dropping the [`PairServer`] closes the
//! socket, which is how "only while its screen is visible" is enforced.
//!
//! Two properties are load-bearing enough to name here, with the details at their definitions:
//! locality is a **peer check**, not a bind ([`server`]); and every served byte goes through
//! one page shell that ends in the AGPL §13 source-code offer ([`form`]), so serving a page
//! without it is not something a future change can forget to do.
//!
//! # Known gap: finding the address to advertise
//!
//! [`PairServer::start`] infers the TV's address by asking the kernel which interface leaves
//! the host. That is correct on a plain LAN and **wrong behind a full-tunnel VPN**, where the
//! route out is the tunnel and the LAN address is on an interface the probe never sees.
//! Measured on a host with Wi-Fi at `192.168.50.98` and a VPN at `100.80.166.175`, every
//! probe destination returned the tunnel; no choice of destination fixes it, because the only
//! one that works is inside the subnet we are trying to discover. Enumerating interfaces
//! would answer it properly, but needs a dependency (`if-addrs`) this crate does not have.
//!
//! So [`PairServer::start_advertising`] takes the address as an argument. A shell that knows
//! better — both do: `NWInterface` on tvOS, `WifiManager` / `NetworkInterface` on Android —
//! supplies it and the gap closes. `start` stays as the convenience path for the common case,
//! and fails loudly rather than advertising an address a phone cannot dial.
//!
//! None of this touches security. Locality is the peer check, which judges the connection in
//! front of it; the URL only affects whether a phone can find the server in the first place.
#![forbid(unsafe_code)]

pub mod error;
pub mod form;
pub mod server;
pub mod token;

pub use error::PairError;
pub use form::Submission;
pub use server::{PairServer, SubmissionSink};
pub use token::PairToken;
