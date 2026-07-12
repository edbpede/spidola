// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Secret types: redacted `Debug`, zeroize-on-drop, never serde-serialize the raw value
//! (TECH_SPEC §12).
//!
//! A [`Secret`] is the only in-memory carrier for credential material (Xtream passwords,
//! token-bearing header values). It exists between the host-secrets callback and the point
//! of use; it is never persisted (the DB stores a [`crate::ids::SecretRef`] instead) and
//! its bytes are wiped when dropped. Because it implements neither `Serialize` nor
//! `Display`, a secret cannot be logged or serialized by accident — the CI grep in
//! `core-api` backs this up by flagging `{:?}` formatting of secret types (§4.8).

use zeroize::{Zeroize, ZeroizeOnDrop};

/// A credential value that redacts its own debug output and zeroizes on drop.
///
/// Deliberately does **not** derive `Clone` (duplicating a secret in memory is a smell) or
/// any serde trait. Access the raw value only through [`Secret::expose`], at the moment of
/// use, and never bind it to a name that outlives that use.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct Secret {
    inner: String,
}

impl Secret {
    /// Wraps a secret value. The input `String`'s buffer is moved in and will be zeroized
    /// on drop; callers holding their own copy should zeroize it themselves.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            inner: value.into(),
        }
    }

    /// Borrows the raw secret for the duration of a single use.
    ///
    /// The returned slice must not be copied into a longer-lived owner, logged, or
    /// serialized. This is the one sanctioned way to read the value.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.inner
    }

    /// Whether the secret is empty (a blank credential is usually a bug upstream).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret([REDACTED])")
    }
}

/// Constant-time-ish equality on the raw bytes, so callers can compare without exposing.
///
/// Not intended as a cryptographic constant-time primitive; it merely avoids handing the
/// raw value out for a `==` check.
impl PartialEq for Secret {
    fn eq(&self, other: &Self) -> bool {
        self.inner.as_bytes() == other.inner.as_bytes()
    }
}

impl Eq for Secret {}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn debug_output_is_redacted() {
        let secret = Secret::new("hunter2-super-secret");
        let rendered = format!("{secret:?}");
        assert_eq!(rendered, "Secret([REDACTED])");
        assert!(
            !rendered.contains("hunter2"),
            "debug output leaked the secret"
        );
    }

    #[test]
    fn debug_of_containing_struct_stays_redacted() {
        #[derive(Debug)]
        struct Holder {
            #[allow(dead_code)]
            token: Secret,
        }
        let holder = Holder {
            token: Secret::new("bearer-abcdef"),
        };
        let rendered = format!("{holder:?}");
        assert!(
            !rendered.contains("abcdef"),
            "nested debug leaked the secret: {rendered}"
        );
    }

    #[test]
    fn expose_returns_the_raw_value() {
        let secret = Secret::new("p@ss");
        assert_eq!(secret.expose(), "p@ss");
        assert!(!secret.is_empty());
    }

    #[test]
    fn equality_compares_bytes_without_exposing() {
        assert_eq!(Secret::new("a"), Secret::new("a"));
        assert_ne!(Secret::new("a"), Secret::new("b"));
    }
}
