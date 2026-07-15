// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Authenticated at-rest protection for M3U catalog values (TECH_SPEC §12).
//!
//! M3U is an unstructured format: credentials can appear in a URL path, query, or header and no
//! heuristic can prove which substring is secret. The core therefore seals the complete locator
//! and every header value before either reaches the staging or live SQLite database. The one
//! random catalog key rests in the platform secure store; SQLite and navigation state carry only
//! an opaque, authenticated envelope. The playable URL is recovered only by `resolve_stream`.

use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use core_model::ids::ChannelIdentity;
use core_model::locator::StreamLocator;
use core_model::secret::Secret;
use rand::Rng as _;
use ring::aead::{self, Aad, LessSafeKey, Nonce, UnboundKey};
use ring::hmac;
use zeroize::Zeroizing;

use crate::error::ApiError;
use crate::secrets::SecretStore;

const KEY_REF: &str = "spidola/catalog/v1/key";
const ENVELOPE_PREFIX: &str = "spidola-sealed://v1/";
const KEY_BYTES: usize = 32;
const NONCE_BYTES: usize = 12;
const LOCATOR_AAD: &[u8] = b"spidola catalog locator v1";
const VALUE_AAD: &[u8] = b"spidola catalog header value v1";
const IDENTITY_TVG_DOMAIN: &[u8] = b"spidola m3u identity tvg-id v1\0";
const IDENTITY_URL_NAME_DOMAIN: &[u8] = b"spidola m3u identity url-name v1\0";

/// Shared catalog cipher, lazily initialized so Xtream-only installations do not create an
/// unrelated secure-store entry. Initialization is serialized: concurrent first imports cannot
/// race two different keys into storage.
pub(crate) struct CatalogCipher {
    secrets: Arc<dyn SecretStore>,
    key: OnceLock<Secret>,
    initialize: Mutex<()>,
}

impl CatalogCipher {
    pub(crate) fn new(secrets: Arc<dyn SecretStore>) -> Arc<Self> {
        Arc::new(Self {
            secrets,
            key: OnceLock::new(),
            initialize: Mutex::new(()),
        })
    }

    /// Seals a locator unless it is already an internal envelope.
    pub(crate) fn seal_locator(&self, locator: &StreamLocator) -> Result<StreamLocator, ApiError> {
        if locator.as_str().starts_with(ENVELOPE_PREFIX) {
            return Ok(locator.clone());
        }
        StreamLocator::parse(&self.seal(locator.as_str(), LOCATOR_AAD)?).map_err(ApiError::from)
    }

    /// Opens a required locator envelope. M3U catalog/history values written under schema 2 are
    /// always sealed; accepting plaintext here would let a database edit remove the prefix and
    /// silently downgrade integrity protection.
    pub(crate) fn open_sealed_locator(
        &self,
        locator: &StreamLocator,
    ) -> Result<StreamLocator, ApiError> {
        let encoded = locator
            .as_str()
            .strip_prefix(ENVELOPE_PREFIX)
            .ok_or(ApiError::StorageCorrupt)?;
        let plain = self.open(encoded, LOCATOR_AAD)?;
        StreamLocator::parse(&plain).map_err(ApiError::from)
    }

    /// Seals an arbitrary header value. Header names remain plaintext; values may carry bearer
    /// tokens and follow the same at-rest rule as locators.
    pub(crate) fn seal_value(&self, value: &str) -> Result<String, ApiError> {
        if value.starts_with(ENVELOPE_PREFIX) {
            return Ok(value.to_owned());
        }
        self.seal(value, VALUE_AAD)
    }

    /// Opens a sealed header value, or returns a plaintext non-envelope unchanged. Xtream and
    /// future non-M3U sources may legitimately carry non-secret override values through this
    /// resolver, so the operation is deliberately idempotent across source kinds.
    pub(crate) fn open_value(&self, value: &str) -> Result<String, ApiError> {
        match value.strip_prefix(ENVELOPE_PREFIX) {
            Some(encoded) => self.open(encoded, VALUE_AAD),
            None => Ok(value.to_owned()),
        }
    }

    /// Opens a required header-value envelope for an M3U channel.
    pub(crate) fn open_sealed_value(&self, value: &str) -> Result<String, ApiError> {
        let encoded = value
            .strip_prefix(ENVELOPE_PREFIX)
            .ok_or(ApiError::StorageCorrupt)?;
        self.open(encoded, VALUE_AAD)
    }

    /// Derives the stable channel identity with the catalog key, so a credential-bearing M3U
    /// locator cannot be tested offline against the public identity stored in SQLite. The
    /// length-prefixed fallback remains stable across refreshes while avoiding ambiguous input
    /// concatenation.
    pub(crate) fn m3u_identity(
        &self,
        tvg_id: Option<&str>,
        url: &str,
        name: &str,
    ) -> Result<ChannelIdentity, ApiError> {
        let key_bytes = self.key_bytes()?;
        let key = hmac::Key::new(hmac::HMAC_SHA256, key_bytes.as_slice());
        let mut context = hmac::Context::with_key(&key);
        if let Some(tvg_id) = tvg_id.filter(|value| !value.is_empty()) {
            context.update(IDENTITY_TVG_DOMAIN);
            update_identity_field(&mut context, tvg_id)?;
        } else {
            context.update(IDENTITY_URL_NAME_DOMAIN);
            update_identity_field(&mut context, url)?;
            update_identity_field(&mut context, name)?;
        }
        let digest = context.sign();
        let raw = digest.as_ref()[..size_of::<u64>()]
            .try_into()
            .map_err(|_| ApiError::Internal)?;
        Ok(ChannelIdentity::from_raw(u64::from_le_bytes(raw)))
    }

    fn seal(&self, plain: &str, aad: &'static [u8]) -> Result<String, ApiError> {
        let key = self.less_safe_key()?;
        let mut nonce_bytes = [0_u8; NONCE_BYTES];
        rand::rng().fill(&mut nonce_bytes);
        let nonce = Nonce::assume_unique_for_key(nonce_bytes);
        let mut in_out = plain.as_bytes().to_vec();
        key.seal_in_place_append_tag(nonce, Aad::from(aad), &mut in_out)
            .map_err(|_| ApiError::Internal)?;

        let mut envelope = nonce_bytes.to_vec();
        envelope.extend_from_slice(&in_out);
        Ok(format!(
            "{ENVELOPE_PREFIX}{}",
            URL_SAFE_NO_PAD.encode(envelope)
        ))
    }

    fn open(&self, encoded: &str, aad: &'static [u8]) -> Result<String, ApiError> {
        let envelope = URL_SAFE_NO_PAD
            .decode(encoded)
            .map_err(|_| ApiError::StorageCorrupt)?;
        if envelope.len() <= NONCE_BYTES + aead::CHACHA20_POLY1305.tag_len() {
            return Err(ApiError::StorageCorrupt);
        }
        let (nonce_bytes, ciphertext) = envelope.split_at(NONCE_BYTES);
        let nonce_array: [u8; NONCE_BYTES] = nonce_bytes
            .try_into()
            .map_err(|_| ApiError::StorageCorrupt)?;
        let nonce = Nonce::assume_unique_for_key(nonce_array);
        let mut in_out = ciphertext.to_vec();
        let key = self.less_safe_key()?;
        let plain = key
            .open_in_place(nonce, Aad::from(aad), &mut in_out)
            .map_err(|_| ApiError::StorageCorrupt)?;
        String::from_utf8(plain.to_vec()).map_err(|_| ApiError::StorageCorrupt)
    }

    fn less_safe_key(&self) -> Result<LessSafeKey, ApiError> {
        let bytes = self.key_bytes()?;
        let unbound = UnboundKey::new(&aead::CHACHA20_POLY1305, bytes.as_slice())
            .map_err(|_| ApiError::Internal)?;
        Ok(LessSafeKey::new(unbound))
    }

    fn key_bytes(&self) -> Result<Zeroizing<Vec<u8>>, ApiError> {
        let bytes = Zeroizing::new(
            URL_SAFE_NO_PAD
                .decode(self.encoded_key()?)
                .map_err(|_| ApiError::StorageCorrupt)?,
        );
        if bytes.len() != KEY_BYTES {
            return Err(ApiError::StorageCorrupt);
        }
        Ok(bytes)
    }

    fn encoded_key(&self) -> Result<&str, ApiError> {
        if let Some(key) = self.key.get() {
            return Ok(key.expose());
        }
        let _guard = self
            .initialize
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(key) = self.key.get() {
            return Ok(key.expose());
        }

        let encoded = if let Some(stored) = self.secrets.get(KEY_REF.to_owned())? {
            let decoded = URL_SAFE_NO_PAD
                .decode(&stored)
                .map_err(|_| ApiError::StorageCorrupt)?;
            if decoded.len() != KEY_BYTES {
                return Err(ApiError::StorageCorrupt);
            }
            stored
        } else {
            let mut bytes = [0_u8; KEY_BYTES];
            rand::rng().fill(&mut bytes);
            let generated = URL_SAFE_NO_PAD.encode(bytes);
            self.secrets.set(KEY_REF.to_owned(), generated.clone())?;
            bytes.fill(0);
            generated
        };
        let _ = self.key.set(Secret::new(encoded));
        self.key.get().map(Secret::expose).ok_or(ApiError::Internal)
    }
}

fn update_identity_field(context: &mut hmac::Context, value: &str) -> Result<(), ApiError> {
    let length = u64::try_from(value.len()).map_err(|_| ApiError::Internal)?;
    context.update(&length.to_le_bytes());
    context.update(value.as_bytes());
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use std::collections::HashMap;

    use super::*;

    #[derive(Default)]
    struct MemorySecrets(Mutex<HashMap<String, String>>);

    impl SecretStore for MemorySecrets {
        fn get(&self, key: String) -> Result<Option<String>, ApiError> {
            Ok(self.0.lock().unwrap().get(&key).cloned())
        }

        fn set(&self, key: String, value: String) -> Result<(), ApiError> {
            self.0.lock().unwrap().insert(key, value);
            Ok(())
        }

        fn delete(&self, key: String) -> Result<(), ApiError> {
            self.0.lock().unwrap().remove(&key);
            Ok(())
        }
    }

    #[test]
    fn locator_round_trips_without_plaintext_in_the_envelope() {
        let cipher = CatalogCipher::new(Arc::new(MemorySecrets::default()));
        let raw = StreamLocator::parse("http://host/user/password/1.ts?token=secret").unwrap();
        let sealed = cipher.seal_locator(&raw).unwrap();
        assert!(sealed.as_str().starts_with(ENVELOPE_PREFIX));
        assert!(!sealed.as_str().contains("password"));
        assert_eq!(cipher.open_sealed_locator(&sealed).unwrap(), raw);
    }

    #[test]
    fn tampering_is_rejected() {
        let cipher = CatalogCipher::new(Arc::new(MemorySecrets::default()));
        let raw = StreamLocator::parse("http://host/secret/1.ts").unwrap();
        let sealed = cipher.seal_locator(&raw).unwrap();
        let mut damaged = sealed.as_str().to_owned();
        damaged.push('A');
        let damaged = StreamLocator::parse(&damaged).unwrap();
        assert_eq!(
            cipher.open_sealed_locator(&damaged),
            Err(ApiError::StorageCorrupt)
        );
    }

    #[test]
    fn removing_the_envelope_prefix_cannot_downgrade_integrity() {
        let cipher = CatalogCipher::new(Arc::new(MemorySecrets::default()));
        let raw = StreamLocator::parse("http://host/secret/1.ts").unwrap();
        assert_eq!(
            cipher.open_sealed_locator(&raw),
            Err(ApiError::StorageCorrupt)
        );
    }

    #[test]
    fn header_ciphertext_cannot_be_substituted_for_a_locator() {
        let cipher = CatalogCipher::new(Arc::new(MemorySecrets::default()));
        let sealed_header = cipher.seal_value("http://attacker.example/stream").unwrap();
        let substituted = StreamLocator::parse(&sealed_header).unwrap();
        assert_eq!(
            cipher.open_sealed_locator(&substituted),
            Err(ApiError::StorageCorrupt)
        );
    }

    #[test]
    fn m3u_identity_is_stable_with_one_key_and_unusable_without_it() {
        let cipher = CatalogCipher::new(Arc::new(MemorySecrets::default()));
        let other = CatalogCipher::new(Arc::new(MemorySecrets::default()));
        let url = "http://host/account/password/1.ts";

        let first = cipher.m3u_identity(None, url, "News").unwrap();
        assert_eq!(first, cipher.m3u_identity(None, url, "News").unwrap());
        assert_ne!(first, other.m3u_identity(None, url, "News").unwrap());
    }
}
