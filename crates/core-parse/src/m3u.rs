// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The streaming M3U parser (TECH_SPEC §4.2).
//!
//! Bytes are pushed in arbitrary chunks (from a file read or the HTTP body stream); the
//! parser maintains a small state machine over EXTINF / option / URL lines and emits
//! channels in batches through a caller [`ChannelSink`]. It never materializes the whole
//! playlist: peak memory is bounded to one batch plus one line, regardless of playlist
//! size — the property that makes the 50k-channel budget honest on 1 GB devices. It is
//! tolerant by design (unknown attributes preserved, malformed entries skipped-and-counted,
//! oversized lines discarded, encodings sniffed with a UTF-8-lossy fallback).

pub mod attributes;
pub mod diagnostics;
pub mod lexer;

use std::collections::BTreeMap;
use std::mem;

use crate::error::ParseError;
use crate::sink::{ChannelSink, ParsedChannel};

use attributes::{ExtInf, parse_extinf};
use diagnostics::{Diagnostics, SkipReason};
use lexer::{Line, classify, looks_like_url};

/// Default number of channels buffered before a batch is flushed to the sink.
pub const DEFAULT_BATCH_SIZE: usize = 1_000;

/// Upper bound on a single line's length; a line longer than this is discarded so that a
/// pathological newline-free input cannot grow parser memory without bound.
const MAX_LINE_BYTES: usize = 64 * 1024;

const UTF8_BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];

/// A `#EXTINF` entry accumulating option/group lines until its URL arrives.
#[derive(Debug, Default)]
struct Pending {
    name: String,
    duration_secs: Option<f64>,
    attributes: BTreeMap<String, String>,
    user_agent: Option<String>,
    headers: Vec<(String, String)>,
    extgrp: Option<String>,
}

impl Pending {
    fn from_extinf(ext: ExtInf) -> Self {
        Self {
            name: ext.name,
            duration_secs: ext.duration_secs,
            attributes: ext.attributes,
            ..Self::default()
        }
    }

    fn into_channel(mut self, url: &str) -> ParsedChannel {
        // An explicit #EXTGRP fills group-title only when the EXTINF didn't set it.
        if !self.attributes.contains_key("group-title")
            && let Some(group) = self.extgrp.take()
        {
            self.attributes.insert("group-title".to_owned(), group);
        }
        let name = if self.name.is_empty() {
            url.to_owned()
        } else {
            self.name
        };
        ParsedChannel {
            name,
            url: url.to_owned(),
            duration_secs: self.duration_secs,
            attributes: self.attributes,
            user_agent: self.user_agent,
            headers: self.headers,
        }
    }
}

/// The streaming M3U parser. Feed it with [`M3uParser::push`], then call
/// [`M3uParser::finish`] to flush and receive the [`Diagnostics`] ledger.
#[derive(Debug)]
pub struct M3uParser {
    batch_size: usize,
    batch: Vec<ParsedChannel>,
    diagnostics: Diagnostics,
    pending: Option<Pending>,
    line_buf: Vec<u8>,
    started: bool,
    line_overflow: bool,
}

impl Default for M3uParser {
    fn default() -> Self {
        Self::with_batch_size(DEFAULT_BATCH_SIZE)
    }
}

impl M3uParser {
    /// A parser with the default batch size.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A parser flushing every `batch_size` channels (`>= 1`).
    #[must_use]
    pub fn with_batch_size(batch_size: usize) -> Self {
        let batch_size = batch_size.max(1);
        Self {
            batch_size,
            batch: Vec::with_capacity(batch_size),
            diagnostics: Diagnostics::default(),
            pending: None,
            line_buf: Vec::new(),
            started: false,
            line_overflow: false,
        }
    }

    /// Feeds a chunk of bytes. Complete lines are parsed immediately; a trailing partial
    /// line is buffered for the next push.
    ///
    /// # Errors
    /// Returns [`ParseError::Sink`] if flushing a full batch to the sink fails.
    pub fn push<S: ChannelSink>(
        &mut self,
        bytes: &[u8],
        sink: &mut S,
    ) -> Result<(), ParseError<S::Error>> {
        let mut input = bytes;
        if !self.started {
            self.started = true;
            if let Some(stripped) = input.strip_prefix(&UTF8_BOM) {
                input = stripped;
            }
        }
        let mut start = 0;
        for (i, &b) in input.iter().enumerate() {
            if b == b'\n' {
                self.append_bounded(&input[start..i]);
                self.process_line(sink)?;
                start = i + 1;
            }
        }
        self.append_bounded(&input[start..]);
        Ok(())
    }

    /// Flushes the final partial line and any pending entry, returning the diagnostics.
    ///
    /// # Errors
    /// Returns [`ParseError::Sink`] if flushing the remaining batch fails.
    pub fn finish<S: ChannelSink>(
        mut self,
        sink: &mut S,
    ) -> Result<Diagnostics, ParseError<S::Error>> {
        if !self.line_buf.is_empty() || self.line_overflow {
            self.process_line(sink)?;
        }
        if self.pending.take().is_some() {
            self.diagnostics.record_skip(SkipReason::MissingUrl);
        }
        self.flush(sink)?;
        Ok(self.diagnostics)
    }

    fn append_bounded(&mut self, slice: &[u8]) {
        if self.line_overflow {
            return;
        }
        self.line_buf.extend_from_slice(slice);
        if self.line_buf.len() > MAX_LINE_BYTES {
            self.line_overflow = true;
            self.line_buf.clear();
        }
    }

    fn process_line<S: ChannelSink>(&mut self, sink: &mut S) -> Result<(), ParseError<S::Error>> {
        if self.line_overflow {
            self.diagnostics.record_skip(SkipReason::OversizedLine);
            self.line_overflow = false;
            self.line_buf.clear();
            return Ok(());
        }
        // Sniff encoding per line: valid UTF-8 borrows; anything else is lossily decoded.
        // Detach the buffer so the borrowed fast path does not alias `self` across
        // `transition`; the allocation is cleared and returned for reuse afterwards.
        let mut buf = mem::take(&mut self.line_buf);
        let result = {
            let decoded = String::from_utf8_lossy(&buf);
            self.transition(&decoded, sink)
        };
        buf.clear();
        self.line_buf = buf;
        result
    }

    fn transition<S: ChannelSink>(
        &mut self,
        line: &str,
        sink: &mut S,
    ) -> Result<(), ParseError<S::Error>> {
        match classify(line) {
            Line::Header | Line::OtherDirective | Line::Blank => Ok(()),
            Line::ExtInf(payload) => {
                if self.pending.take().is_some() {
                    self.diagnostics.record_skip(SkipReason::MissingUrl);
                }
                self.pending = Some(Pending::from_extinf(parse_extinf(payload)));
                Ok(())
            }
            Line::VlcOpt(payload) => {
                if let Some(pending) = self.pending.as_mut() {
                    apply_vlcopt(pending, payload);
                }
                Ok(())
            }
            Line::Group(name) => {
                if let Some(pending) = self.pending.as_mut() {
                    pending.extgrp = Some(name.to_owned());
                }
                Ok(())
            }
            Line::Url(text) => self.emit_url(text, sink),
        }
    }

    fn emit_url<S: ChannelSink>(
        &mut self,
        text: &str,
        sink: &mut S,
    ) -> Result<(), ParseError<S::Error>> {
        if let Some(pending) = self.pending.take() {
            let channel = pending.into_channel(text);
            self.push_channel(channel, sink)?;
            self.diagnostics.record_emitted();
        } else if looks_like_url(text) {
            self.push_channel(bare_channel(text), sink)?;
            self.diagnostics.record_emitted();
        } else {
            self.diagnostics.record_skip(SkipReason::StrayLine);
        }
        Ok(())
    }

    fn push_channel<S: ChannelSink>(
        &mut self,
        channel: ParsedChannel,
        sink: &mut S,
    ) -> Result<(), ParseError<S::Error>> {
        self.batch.push(channel);
        if self.batch.len() >= self.batch_size {
            self.flush(sink)?;
        }
        Ok(())
    }

    fn flush<S: ChannelSink>(&mut self, sink: &mut S) -> Result<(), ParseError<S::Error>> {
        if !self.batch.is_empty() {
            let batch = mem::take(&mut self.batch);
            sink.accept(batch).map_err(ParseError::Sink)?;
        }
        Ok(())
    }
}

fn bare_channel(url: &str) -> ParsedChannel {
    ParsedChannel {
        name: url.to_owned(),
        url: url.to_owned(),
        duration_secs: None,
        attributes: BTreeMap::new(),
        user_agent: None,
        headers: Vec::new(),
    }
}

fn apply_vlcopt(pending: &mut Pending, payload: &str) {
    let Some((key, value)) = payload.split_once('=') else {
        return;
    };
    match key.trim() {
        "http-user-agent" => pending.user_agent = Some(value.to_owned()),
        "http-referrer" => pending
            .headers
            .push(("Referer".to_owned(), value.to_owned())),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use std::convert::Infallible;

    /// A sink that collects every channel and tracks the largest batch it ever received.
    #[derive(Default)]
    struct Collector {
        channels: Vec<ParsedChannel>,
        max_batch: usize,
    }

    impl ChannelSink for Collector {
        type Error = Infallible;
        fn accept(&mut self, batch: Vec<ParsedChannel>) -> Result<(), Self::Error> {
            self.max_batch = self.max_batch.max(batch.len());
            self.channels.extend(batch);
            Ok(())
        }
    }

    fn parse_all(input: &[u8], batch_size: usize) -> (Vec<ParsedChannel>, Diagnostics, usize) {
        let mut sink = Collector::default();
        let mut parser = M3uParser::with_batch_size(batch_size);
        parser.push(input, &mut sink).unwrap();
        let diag = parser.finish(&mut sink).unwrap();
        (sink.channels, diag, sink.max_batch)
    }

    #[test]
    fn parses_a_basic_playlist() {
        let input = b"#EXTM3U\n#EXTINF:-1 tvg-id=\"a\" group-title=\"News\",Alpha\nhttp://a/1\n\
                      #EXTINF:-1,Beta\nhttp://b/2\n";
        let (channels, diag, _) = parse_all(input, 1000);
        assert_eq!(channels.len(), 2);
        assert_eq!(channels[0].name, "Alpha");
        assert_eq!(channels[0].tvg_id(), Some("a"));
        assert_eq!(channels[0].group(), Some("News"));
        assert_eq!(channels[1].url, "http://b/2");
        assert_eq!(diag.emitted(), 2);
        assert_eq!(diag.skipped(), 0);
        assert!(diag.is_balanced());
    }

    #[test]
    fn extgrp_and_vlcopt_attach_to_pending() {
        let input = b"#EXTM3U\n#EXTINF:-1,Chan\n#EXTGRP:Sports\n\
                      #EXTVLCOPT:http-user-agent=SpidolaUA\nhttp://c/3\n";
        let (channels, _, _) = parse_all(input, 1000);
        assert_eq!(channels.len(), 1);
        assert_eq!(channels[0].group(), Some("Sports"));
        assert_eq!(channels[0].user_agent.as_deref(), Some("SpidolaUA"));
    }

    #[test]
    fn tolerates_and_counts_messy_input() {
        let input = b"#EXTM3U\n\
                      #EXTINF:-1,Has URL\nhttp://ok/1\n\
                      #EXTINF:-1,No URL follows\n\
                      #EXTINF:-1,Second\nhttp://ok/2\n\
                      garbage line that is not a url\n\
                      http://bare/3\n";
        let (channels, diag, _) = parse_all(input, 1000);
        // Emitted: Has URL, Second, bare/3.  Skipped: "No URL follows", garbage line.
        assert_eq!(diag.emitted(), 3);
        assert_eq!(diag.skipped(), 2);
        assert_eq!(diag.skips_for(SkipReason::MissingUrl), 1);
        assert_eq!(diag.skips_for(SkipReason::StrayLine), 1);
        assert!(diag.is_balanced());
        assert!(channels.iter().any(|c| c.url == "http://bare/3"));
    }

    #[test]
    fn strips_utf8_bom_and_handles_crlf() {
        let mut input = vec![0xEF, 0xBB, 0xBF];
        input.extend_from_slice(b"#EXTM3U\r\n#EXTINF:-1,BOM\r\nhttp://a/1\r\n");
        let (channels, diag, _) = parse_all(&input, 1000);
        assert_eq!(channels.len(), 1);
        assert_eq!(channels[0].name, "BOM");
        assert!(diag.is_balanced());
    }

    #[test]
    fn invalid_utf8_is_lossily_decoded_not_panicked() {
        // An EXTINF name with an invalid byte (0xFF) must not panic.
        let input = b"#EXTM3U\n#EXTINF:-1,Bad\xffName\nhttp://a/1\n";
        let (channels, diag, _) = parse_all(input, 1000);
        assert_eq!(channels.len(), 1);
        assert!(channels[0].name.contains("Bad"));
        assert!(diag.is_balanced());
    }

    #[test]
    fn peak_memory_is_bounded_to_one_batch() {
        // Feed far more channels than the batch size, one byte at a time (worst case for
        // buffering), and assert the parser never held more than one batch.
        let batch_size = 16;
        let mut input = String::from("#EXTM3U\n");
        for i in 0..500 {
            use std::fmt::Write as _;
            let _ = write!(input, "#EXTINF:-1,Ch{i}\nhttp://h/{i}\n");
        }
        let mut sink = Collector::default();
        let mut parser = M3uParser::with_batch_size(batch_size);
        for byte in input.as_bytes() {
            parser.push(std::slice::from_ref(byte), &mut sink).unwrap();
            assert!(
                parser.batch.len() <= batch_size,
                "live batch exceeded batch_size"
            );
            assert!(
                parser.line_buf.len() <= MAX_LINE_BYTES,
                "line buffer grew unbounded"
            );
        }
        let diag = parser.finish(&mut sink).unwrap();
        assert_eq!(diag.emitted(), 500);
        assert_eq!(sink.channels.len(), 500);
        assert!(sink.max_batch <= batch_size);
    }

    #[test]
    fn oversized_line_is_skipped_not_buffered() {
        let mut input = Vec::from(&b"#EXTM3U\n"[..]);
        input.extend(std::iter::repeat_n(b'x', MAX_LINE_BYTES + 10));
        input.push(b'\n');
        input.extend_from_slice(b"#EXTINF:-1,After\nhttp://a/1\n");
        let (channels, diag, _) = parse_all(&input, 1000);
        assert_eq!(channels.len(), 1);
        assert_eq!(channels[0].name, "After");
        assert_eq!(diag.skips_for(SkipReason::OversizedLine), 1);
        assert!(diag.is_balanced());
    }
}
