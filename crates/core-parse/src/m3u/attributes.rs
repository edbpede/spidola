// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! `#EXTINF` attribute handling with unknown-attribute preservation (TECH_SPEC §4.2).
//!
//! An `#EXTINF` line is `<duration> key="value" key="value",<display name>`. Values are
//! quoted and may contain commas and spaces, so the display-name split is the first comma
//! **outside** quotes. Every `key="value"` pair is preserved — known keys (`tvg-id`,
//! `tvg-logo`, `group-title`, …) and unknown ones alike — so nothing a source declares is
//! silently dropped. Parsing is tolerant: a malformed tail simply stops attribute scanning.

use std::collections::BTreeMap;

/// The parsed content of a `#EXTINF:` payload.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExtInf {
    /// Duration in seconds, if the leading token parsed (`-1` for live).
    pub duration_secs: Option<f64>,
    /// All `key="value"` attributes, in sorted order, unknown keys included.
    pub attributes: BTreeMap<String, String>,
    /// The display name (text after the first unquoted comma), trimmed.
    pub name: String,
}

/// Parses the payload that follows `#EXTINF:`.
#[must_use]
pub fn parse_extinf(payload: &str) -> ExtInf {
    let split = first_unquoted_comma(payload);
    let (head, name) = match split {
        Some(idx) => (&payload[..idx], payload[idx + 1..].trim().to_owned()),
        None => (payload, String::new()),
    };

    // The duration is the first whitespace-delimited token; the rest are attributes.
    let head = head.trim_start();
    let (duration_token, attr_str) = match head.find(char::is_whitespace) {
        Some(idx) => (&head[..idx], &head[idx..]),
        None => (head, ""),
    };
    let duration_secs = duration_token.trim().parse::<f64>().ok();

    ExtInf {
        duration_secs,
        attributes: parse_attributes(attr_str),
        name,
    }
}

/// Index of the first comma that is not inside a double-quoted region.
fn first_unquoted_comma(s: &str) -> Option<usize> {
    let mut in_quotes = false;
    for (idx, ch) in s.char_indices() {
        match ch {
            '"' => in_quotes = !in_quotes,
            ',' if !in_quotes => return Some(idx),
            _ => {}
        }
    }
    None
}

/// Scans `key="value"` pairs, tolerating and stopping at the first malformed run.
fn parse_attributes(mut rest: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    loop {
        rest = rest.trim_start();
        let Some(eq) = rest.find('=') else { break };
        let key = rest[..eq].trim();
        let after = &rest[eq + 1..];
        // Values must be double-quoted; a bare/unterminated value ends the scan.
        let Some(stripped) = after.strip_prefix('"') else {
            break;
        };
        let Some(close) = stripped.find('"') else {
            break;
        };
        let value = &stripped[..close];
        if !key.is_empty() {
            out.insert(key.to_owned(), value.to_owned());
        }
        rest = &stripped[close + 1..];
    }
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn parses_duration_attributes_and_name() {
        let ext = parse_extinf(
            "-1 tvg-id=\"bbc.one\" tvg-logo=\"http://x/l.png\" group-title=\"News\",BBC One HD",
        );
        assert_eq!(ext.duration_secs, Some(-1.0));
        assert_eq!(ext.name, "BBC One HD");
        assert_eq!(ext.attributes.get("tvg-id").unwrap(), "bbc.one");
        assert_eq!(ext.attributes.get("group-title").unwrap(), "News");
    }

    #[test]
    fn preserves_unknown_attributes() {
        let ext = parse_extinf("-1 tvg-id=\"a\" catchup=\"append\" x-custom=\"42\",Name");
        assert_eq!(ext.attributes.get("catchup").unwrap(), "append");
        assert_eq!(ext.attributes.get("x-custom").unwrap(), "42");
    }

    #[test]
    fn handles_commas_inside_quoted_values() {
        let ext = parse_extinf("-1 tvg-name=\"News, Sport & More\",Channel, With Comma");
        assert_eq!(
            ext.attributes.get("tvg-name").unwrap(),
            "News, Sport & More"
        );
        // The display name is everything after the FIRST unquoted comma.
        assert_eq!(ext.name, "Channel, With Comma");
    }

    #[test]
    fn tolerates_missing_name_and_attributes() {
        let ext = parse_extinf("-1");
        assert_eq!(ext.duration_secs, Some(-1.0));
        assert!(ext.attributes.is_empty());
        assert_eq!(ext.name, "");
    }

    #[test]
    fn tolerates_malformed_attribute_tail() {
        // Unterminated quote ends the scan without panicking.
        let ext = parse_extinf("-1 tvg-id=\"ok\" broken=\"unterminated,Name");
        assert_eq!(ext.attributes.get("tvg-id").unwrap(), "ok");
        assert!(!ext.attributes.contains_key("broken"));
    }
}
