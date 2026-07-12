// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Trigram similarity for the fuzzy fallback ranking (TECH_SPEC §4.4, PRD §6.4).
//!
//! When the FTS5 prefix search finds nothing (typo, transposition), the fallback ranks
//! candidate names by character-trigram Jaccard similarity. Pure and unit-tested; the
//! bounded candidate scan that consumes it lives in [`crate::search`]. This never runs on
//! the hot prefix path, so it is a correctness feature, not a budget-critical one.

use std::collections::HashSet;

/// Jaccard similarity (0.0..=1.0) over the character trigrams of two strings.
///
/// Inputs are lowercased and space-normalized, then padded so leading/trailing characters
/// still form trigrams. Identical strings score `1.0`; disjoint strings score `0.0`.
#[must_use]
// Trigram counts are small (bounded by string length), so the usize→f32 cast is exact.
#[allow(clippy::cast_precision_loss)]
pub fn similarity(a: &str, b: &str) -> f32 {
    let ta = trigrams(a);
    let tb = trigrams(b);
    if ta.is_empty() && tb.is_empty() {
        return 1.0;
    }
    if ta.is_empty() || tb.is_empty() {
        return 0.0;
    }
    let intersection = ta.intersection(&tb).count();
    let union = ta.len() + tb.len() - intersection;
    if union == 0 {
        0.0
    } else {
        intersection as f32 / union as f32
    }
}

fn trigrams(s: &str) -> HashSet<(char, char, char)> {
    let normalized: String = normalize(s);
    // Pad so the first and last real characters participate in trigrams.
    let padded: Vec<char> = std::iter::once(' ')
        .chain(std::iter::once(' '))
        .chain(normalized.chars())
        .chain(std::iter::once(' '))
        .collect();
    padded.windows(3).map(|w| (w[0], w[1], w[2])).collect()
}

fn normalize(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_space = false;
    for ch in s.chars().flat_map(char::to_lowercase) {
        if ch.is_whitespace() {
            if !prev_space {
                out.push(' ');
                prev_space = true;
            }
        } else {
            out.push(ch);
            prev_space = false;
        }
    }
    out.trim().to_owned()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp)]

    use super::*;

    #[test]
    fn identical_strings_score_one() {
        assert_eq!(similarity("BBC One", "bbc one"), 1.0);
    }

    #[test]
    fn disjoint_strings_score_zero() {
        assert_eq!(similarity("aaaa", "zzzz"), 0.0);
    }

    #[test]
    fn typos_score_between_zero_and_one() {
        let s = similarity("discovery", "discvoery"); // transposed
        assert!(s > 0.3 && s < 1.0, "unexpected score {s}");
    }

    #[test]
    fn closer_match_ranks_higher() {
        let close = similarity("national geographic", "national geografic");
        let far = similarity("national geographic", "cartoon network");
        assert!(close > far);
    }
}
