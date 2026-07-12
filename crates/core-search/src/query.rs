// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Prefix query compilation over FTS5 (TECH_SPEC §4.4, PRD §6.4).
//!
//! The instant-search path compiles user text into an FTS5 `MATCH` expression: each
//! whitespace/punctuation-delimited term becomes a prefix token (`term*`) and terms are
//! combined with AND. Splitting on non-alphanumerics also sanitizes the input, so FTS5
//! raw query can't be injected. Pure and allocation-cheap; the execution lives in
//! [`crate::search`].

/// Compiles user text into an FTS5 prefix `MATCH` expression.
///
/// Returns `None` when the text has no usable term (empty or punctuation-only), in which
/// case the caller should not run a prefix query.
#[must_use]
pub fn compile_match(text: &str) -> Option<String> {
    let mut expr = String::new();
    for term in text.split(|c: char| !c.is_alphanumeric()) {
        if term.is_empty() {
            continue;
        }
        if !expr.is_empty() {
            expr.push(' ');
        }
        // Lowercasing is redundant with unicode61 folding but keeps the expression tidy.
        expr.push_str(&term.to_lowercase());
        expr.push('*');
    }
    if expr.is_empty() { None } else { Some(expr) }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn single_and_multi_term_prefixes() {
        assert_eq!(compile_match("bbc").as_deref(), Some("bbc*"));
        assert_eq!(compile_match("bbc one").as_deref(), Some("bbc* one*"));
    }

    #[test]
    fn punctuation_is_stripped_not_injected() {
        // FTS5 operators / quotes in the raw text cannot reach the MATCH expression.
        assert_eq!(
            compile_match("bbc OR* \"one\"").as_deref(),
            Some("bbc* or* one*")
        );
        assert_eq!(
            compile_match("news: sport!").as_deref(),
            Some("news* sport*")
        );
    }

    #[test]
    fn empty_and_punctuation_only_yield_none() {
        assert!(compile_match("").is_none());
        assert!(compile_match("   ").is_none());
        assert!(compile_match("!!!").is_none());
    }

    #[test]
    fn unicode_terms_are_preserved() {
        assert_eq!(compile_match("Zürich").as_deref(), Some("zürich*"));
    }
}
