// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Newtype identifiers (`SourceId`, `ChannelId`, …) so cross-wiring is a compile error.
//!
//! Every persisted aggregate is keyed by a newtype over the SQLite rowid rather than a
//! bare `i64`, so passing a `SourceId` where a `ChannelId` is expected does not compile
//! (rust-dev-pro.md "newtype" idiom; TECH_SPEC §4.1). `SecretRef` is the opaque
//! host-secrets key stored in the DB in place of any credential (§12).

use serde::{Deserialize, Serialize};

/// Declares a `#[repr(transparent)]`-style rowid newtype with a uniform surface.
macro_rules! rowid_newtype {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(i64);

        impl $name {
            /// Wraps a raw rowid. Rowids originate from `core-db`; the domain never mints them.
            #[must_use]
            pub const fn new(value: i64) -> Self {
                Self(value)
            }

            /// The underlying rowid, for the persistence layer only.
            #[must_use]
            pub const fn value(self) -> i64 {
                self.0
            }
        }

        impl From<i64> for $name {
            fn from(value: i64) -> Self {
                Self(value)
            }
        }

        impl From<$name> for i64 {
            fn from(id: $name) -> Self {
                id.0
            }
        }
    };
}

rowid_newtype!(
    /// Identity of a configured source (M3U URL / M3U file / Xtream account).
    SourceId
);
rowid_newtype!(
    /// Rowid of a channel within the current catalog snapshot.
    ///
    /// Not stable across a refresh — that role belongs to [`ChannelIdentity`].
    ChannelId
);
rowid_newtype!(
    /// Rowid of a category / group within a source.
    CategoryId
);
rowid_newtype!(
    /// Rowid of a stored EPG programme entry.
    EpgEntryId
);
rowid_newtype!(
    /// Rowid of a playback-history record.
    HistoryId
);

/// A **stable** per-source channel identity.
///
/// Rowids churn on every refresh (staging-and-swap), so favorites and hidden flags key on
/// this content-derived hash instead, letting them survive a refresh (TECH_SPEC §4.4).
/// Construction lives in [`crate::channel`]; the type is opaque here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ChannelIdentity(u64);

impl ChannelIdentity {
    /// Wraps a precomputed identity value. Prefer [`crate::channel::channel_identity`].
    #[must_use]
    pub const fn from_raw(value: u64) -> Self {
        Self(value)
    }

    /// The raw identity value. Stored in SQLite as the bit-equivalent `i64`.
    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }

    /// Bit-cast to the `i64` SQLite stores (SQLite integers are signed 64-bit).
    #[must_use]
    #[allow(clippy::cast_possible_wrap)]
    pub const fn to_storage(self) -> i64 {
        self.0 as i64
    }

    /// Inverse of [`Self::to_storage`].
    #[must_use]
    #[allow(clippy::cast_sign_loss)]
    pub const fn from_storage(value: i64) -> Self {
        Self(value as u64)
    }
}

/// An opaque key into host secure storage (Keychain / Keystore), stored in the DB in place
/// of any secret value (TECH_SPEC §12). Holds no credential material itself.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SecretRef(String);

impl SecretRef {
    /// Wraps an opaque host-secrets key. The key names a secret; it is not the secret.
    #[must_use]
    pub fn new(key: impl Into<String>) -> Self {
        Self(key.into())
    }

    /// The opaque key as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn rowid_roundtrips_through_i64() {
        let id = SourceId::new(42);
        assert_eq!(id.value(), 42);
        assert_eq!(i64::from(id), 42);
        assert_eq!(SourceId::from(42), id);
    }

    #[test]
    fn identity_bit_casts_are_inverse() {
        let identity = ChannelIdentity::from_raw(u64::MAX);
        assert_eq!(identity.to_storage(), -1);
        assert_eq!(ChannelIdentity::from_storage(-1), identity);
    }

    #[test]
    fn secret_ref_is_just_a_key() {
        let key = SecretRef::new("xtream/1/password");
        assert_eq!(key.as_str(), "xtream/1/password");
    }
}
