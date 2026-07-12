// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Line classifier for the M3U state machine (TECH_SPEC §4.2).
//!
//! Classification is pure and allocation-free: it borrows into the decoded line and names
//! what kind of line it is. The transition logic (accumulating a pending entry, emitting on
//! a URL) lives in the [`crate::m3u`] parser; keeping the two apart makes both testable.

/// One classified M3U line. Payload slices borrow from the input line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Line<'a> {
    /// The `#EXTM3U` header (with optional attributes we currently ignore).
    Header,
    /// `#EXTINF:<payload>` — duration, attributes, and display name.
    ExtInf(&'a str),
    /// `#EXTVLCOPT:<payload>` — a `key=value` player option.
    VlcOpt(&'a str),
    /// `#EXTGRP:<name>` — an explicit group for the pending entry.
    Group(&'a str),
    /// Any other `#EXT…`/`#`-comment directive (HLS tags, SPDX comments, …); ignored.
    OtherDirective,
    /// A blank line.
    Blank,
    /// A non-directive line — a candidate stream URL.
    Url(&'a str),
}

/// Classifies one already-decoded, `\r`-trimmed line.
#[must_use]
pub fn classify(line: &str) -> Line<'_> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Line::Blank;
    }
    if let Some(rest) = trimmed.strip_prefix('#') {
        // `rest` is the directive without the leading '#'.
        if rest == "EXTM3U" || rest.starts_with("EXTM3U ") {
            return Line::Header;
        }
        if let Some(payload) = rest.strip_prefix("EXTINF:") {
            return Line::ExtInf(payload);
        }
        if let Some(payload) = rest.strip_prefix("EXTVLCOPT:") {
            return Line::VlcOpt(payload);
        }
        if let Some(payload) = rest.strip_prefix("EXTGRP:") {
            return Line::Group(payload.trim());
        }
        return Line::OtherDirective;
    }
    Line::Url(trimmed)
}

/// Whether a bare (EXTINF-less) line looks enough like a stream URL to emit.
#[must_use]
pub fn looks_like_url(text: &str) -> bool {
    text.contains("://")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn classifies_directives_and_urls() {
        assert_eq!(classify("#EXTM3U"), Line::Header);
        assert_eq!(classify("#EXTM3U x-tvg-url=\"y\""), Line::Header);
        assert_eq!(classify("#EXTINF:-1,Name"), Line::ExtInf("-1,Name"));
        assert_eq!(
            classify("#EXTVLCOPT:http-user-agent=UA"),
            Line::VlcOpt("http-user-agent=UA")
        );
        assert_eq!(classify("#EXTGRP: News "), Line::Group("News"));
        assert_eq!(classify("#EXT-X-VERSION:3"), Line::OtherDirective);
        assert_eq!(
            classify("# SPDX-License-Identifier: X"),
            Line::OtherDirective
        );
        assert_eq!(classify("   "), Line::Blank);
        assert_eq!(classify("http://a/b.ts"), Line::Url("http://a/b.ts"));
    }

    #[test]
    fn url_heuristic() {
        assert!(looks_like_url("http://a/b"));
        assert!(looks_like_url("rtmp://a/b"));
        assert!(!looks_like_url("just some text"));
    }
}
