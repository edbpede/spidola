// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Session-random submission token — the capability that gates every POST (TECH_SPEC §12).
//!
//! The token is minted per server start, lives only in memory, and dies with the screen. It
//! is never persisted, never logged, and never travels anywhere except onto the TV for a
//! person to read. That is the whole mechanism behind "a person on the network cannot inject
//! a source into a TV they cannot see": the token's distribution channel *is* line of sight.
//!
//! Which makes its length a UX constraint before it is a security one. A human reads this
//! off a television across a room and types it on a phone (PRD §6.1), so it is six
//! characters from an alphabet with no letter/digit lookalikes — roughly 29 bits. That is
//! nowhere near a key, and does not need to be: an attacker must already be inside the LAN
//! to be answered at all (the peer check in [`crate::server`]), and the server exists only
//! while a person is looking at the screen. Six characters is what a person will actually
//! type; the peer check and the lifetime bound are what make six characters enough.

use rand::Rng;

/// Characters a person can read off a TV and type on a phone without a second guess.
///
/// Crockford's base32 alphabet: digits and uppercase letters, minus `0`/`O` and `1`/`I`/`L`
/// (the lookalike pairs), minus `U` (which Crockford drops so a random string cannot spell
/// something regrettable). 30 symbols — deliberately not a power of two, because
/// unambiguity beats a tidy bit count here.
const ALPHABET: &[u8] = b"23456789ABCDEFGHJKMNPQRSTVWXYZ";

/// Token length in characters. See the module docs: set by what a person will actually type,
/// not by a bit target.
const TOKEN_LEN: usize = 6;

/// The session's submission token.
///
/// A capability, so it is treated like one: [`Debug`] is redacted and there is exactly one
/// sanctioned way to read the value — [`PairToken::display`], named for its single purpose.
/// This mirrors the `Secret::expose` idiom in `core_model::secret`; the difference is that a
/// token is *meant* to be shown to a person, so the accessor says so rather than pretending
/// otherwise.
pub struct PairToken {
    inner: String,
}

impl PairToken {
    /// Mints a fresh token for one server session.
    ///
    /// Draws from `rand::rng()`, which is a CSPRNG (OS-seeded `ChaCha12`) — load-bearing, not
    /// incidental: a predictable token would let someone who never saw the screen submit
    /// anyway, which is the exact property this type exists to prevent.
    #[must_use]
    pub fn generate() -> Self {
        let mut rng = rand::rng();
        let inner = (0..TOKEN_LEN)
            .map(|_| char::from(ALPHABET[rng.random_range(0..ALPHABET.len())]))
            .collect();
        Self { inner }
    }

    /// The token text, for the one caller allowed to have it: the TV screen.
    ///
    /// The shell renders this beside the pairing URL and QR code. It must not reach a log, a
    /// file, or the served HTML — putting it on the wire would hand it to exactly the person
    /// it exists to exclude.
    #[must_use]
    pub fn display(&self) -> &str {
        &self.inner
    }

    /// Whether a submitted string is this session's token.
    ///
    /// Accepts what a phone actually sends: surrounding whitespace from a paste, and any
    /// case, since a soft keyboard's autocapitalize is not the user's decision to make. The
    /// alphabet is uppercase-only, so folding the candidate up before comparing costs no
    /// entropy — it only stops a correct answer from being rejected on presentation.
    #[must_use]
    pub fn matches(&self, candidate: &str) -> bool {
        let normalized = candidate.trim().to_ascii_uppercase();
        constant_time_eq(self.inner.as_bytes(), normalized.as_bytes())
    }
}

impl std::fmt::Debug for PairToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PairToken([REDACTED])")
    }
}

/// Byte equality that does not short-circuit on the first difference.
///
/// Not a cryptographic constant-time primitive — the length check returns early, and nothing
/// here stops a compiler from reintroducing a branch. It refuses only the obvious tell: `==`
/// on a wrong first character returns measurably sooner than on a wrong last one, which over
/// enough tries leaks the token a character at a time. Length is not secret (every token is
/// [`TOKEN_LEN`] characters), so returning early on it costs nothing.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0_u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use std::collections::HashSet;

    #[test]
    fn tokens_are_typeable_off_a_screen() {
        let token = PairToken::generate();
        let text = token.display();
        assert_eq!(text.chars().count(), TOKEN_LEN);
        for ch in text.chars() {
            assert!(
                ALPHABET.contains(&u8::try_from(ch).unwrap()),
                "token contains {ch:?}, which is not in the unambiguous alphabet"
            );
        }
    }

    #[test]
    fn the_alphabet_excludes_every_lookalike() {
        for ch in "01OILU".bytes() {
            assert!(
                !ALPHABET.contains(&ch),
                "{:?} is a lookalike and must not be drawable",
                char::from(ch)
            );
        }
    }

    #[test]
    fn tokens_are_session_random() {
        // 30^6 ≈ 7.3e8, so 64 draws colliding means the generator is broken, not unlucky.
        let drawn: HashSet<String> = (0..64)
            .map(|_| PairToken::generate().display().to_owned())
            .collect();
        assert_eq!(drawn.len(), 64, "generate() repeated itself");
    }

    #[test]
    fn debug_output_is_redacted() {
        let token = PairToken::generate();
        let text = token.display().to_owned();
        let rendered = format!("{token:?}");
        assert_eq!(rendered, "PairToken([REDACTED])");
        assert!(
            !rendered.contains(&text),
            "debug output leaked the token: {rendered}"
        );
    }

    #[test]
    fn debug_of_containing_struct_stays_redacted() {
        // The realistic leak is not `{:?}` on the token — it is `{:?}` on whatever holds it.
        #[derive(Debug)]
        struct Holder {
            #[allow(dead_code)]
            token: PairToken,
        }
        let token = PairToken::generate();
        let text = token.display().to_owned();
        let rendered = format!("{:?}", Holder { token });
        assert!(
            !rendered.contains(&text),
            "nested debug leaked the token: {rendered}"
        );
    }

    #[test]
    fn matches_accepts_what_a_phone_actually_sends() {
        let token = PairToken::generate();
        let text = token.display().to_owned();
        assert!(token.matches(&text), "exact token must match");
        assert!(
            token.matches(&text.to_ascii_lowercase()),
            "autocapitalize must not lock the user out"
        );
        assert!(
            token.matches(&format!("  {text} ")),
            "a paste may carry whitespace"
        );
    }

    #[test]
    fn matches_rejects_everything_else() {
        let token = PairToken::generate();
        let text = token.display().to_owned();
        assert!(!token.matches(""), "an empty submission must not pass");
        assert!(
            !token.matches(&text[..TOKEN_LEN - 1]),
            "a prefix must not pass"
        );
        assert!(
            !token.matches(&format!("{text}X")),
            "a superstring must not pass"
        );
        // Flip the first character to a different alphabet symbol.
        let mut wrong = text.into_bytes();
        wrong[0] = if wrong[0] == ALPHABET[0] {
            ALPHABET[1]
        } else {
            ALPHABET[0]
        };
        let wrong = String::from_utf8(wrong).unwrap();
        assert!(!token.matches(&wrong), "a one-character miss must not pass");
    }

    #[test]
    fn one_token_does_not_answer_for_another() {
        let a = PairToken::generate();
        let b = PairToken::generate();
        assert!(
            !a.matches(b.display()),
            "tokens must not be interchangeable"
        );
    }

    #[test]
    fn constant_time_eq_agrees_with_equality() {
        assert!(constant_time_eq(b"ABC234", b"ABC234"));
        assert!(!constant_time_eq(b"ABC234", b"XBC234")); // differs first
        assert!(!constant_time_eq(b"ABC234", b"ABC23X")); // differs last
        assert!(!constant_time_eq(b"ABC234", b"ABC23")); // differs in length
        assert!(constant_time_eq(b"", b""));
    }
}
