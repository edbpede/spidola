// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! A small streaming XML pull tokenizer for the XMLTV subset Spidola consumes.
//!
//! It accepts arbitrary byte chunks, never assumes UTF-8 validity, and emits one event at a
//! time through a callback so neither the input nor an event list is accumulated in memory.

use std::mem;

/// Maximum bytes retained for one XML tag or text/CDATA run.
pub const MAX_XML_TOKEN_BYTES: usize = 64 * 1024;

/// One decoded XML event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PullEvent {
    /// An opening element and its decoded attributes.
    Start {
        /// Local element name (namespace prefix removed).
        name: String,
        /// Local attribute names and decoded values.
        attributes: Vec<(String, String)>,
    },
    /// A closing element.
    End {
        /// Local element name (namespace prefix removed).
        name: String,
    },
    /// Decoded character data.
    Text(String),
    /// Malformed markup was discarded.
    Malformed,
    /// A tag or text run exceeded [`MAX_XML_TOKEN_BYTES`] and was discarded.
    Oversized,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Text,
    Tag,
    Comment,
    Cdata,
}

/// Incremental XML tokenizer. Feed chunks with [`Self::push`] and close with [`Self::finish`].
#[derive(Debug)]
pub struct PullParser {
    mode: Mode,
    buffer: Vec<u8>,
    quote: Option<u8>,
    overflow: bool,
    max_buffered: usize,
}

impl Default for PullParser {
    fn default() -> Self {
        Self {
            mode: Mode::Text,
            buffer: Vec::new(),
            quote: None,
            overflow: false,
            max_buffered: 0,
        }
    }
}

impl PullParser {
    /// Feeds arbitrary bytes and emits decoded events synchronously.
    ///
    /// # Errors
    /// Returns the callback's error immediately.
    pub fn push<E>(
        &mut self,
        bytes: &[u8],
        emit: &mut impl FnMut(PullEvent) -> Result<(), E>,
    ) -> Result<(), E> {
        for &byte in bytes {
            match self.mode {
                Mode::Text => self.push_text(byte, emit)?,
                Mode::Tag => self.push_tag(byte, emit)?,
                Mode::Comment => self.push_comment(byte),
                Mode::Cdata => self.push_cdata(byte, emit)?,
            }
        }
        Ok(())
    }

    /// Flushes final text or reports a truncated construct.
    ///
    /// # Errors
    /// Returns the callback's error immediately.
    pub fn finish<E>(mut self, emit: &mut impl FnMut(PullEvent) -> Result<(), E>) -> Result<(), E> {
        match self.mode {
            Mode::Text => self.emit_buffered_text(emit)?,
            Mode::Tag | Mode::Comment | Mode::Cdata => emit(PullEvent::Malformed)?,
        }
        Ok(())
    }

    /// Largest token buffer observed, used to prove the memory ceiling in tests.
    #[must_use]
    pub fn max_buffered_bytes(&self) -> usize {
        self.max_buffered
    }

    fn push_text<E>(
        &mut self,
        byte: u8,
        emit: &mut impl FnMut(PullEvent) -> Result<(), E>,
    ) -> Result<(), E> {
        if byte == b'<' {
            self.emit_buffered_text(emit)?;
            self.mode = Mode::Tag;
            return Ok(());
        }
        self.append_bounded(byte);
        Ok(())
    }

    fn push_tag<E>(
        &mut self,
        byte: u8,
        emit: &mut impl FnMut(PullEvent) -> Result<(), E>,
    ) -> Result<(), E> {
        if self.overflow {
            if let Some(quote) = self.quote {
                if byte == quote {
                    self.quote = None;
                }
                return Ok(());
            }
            if byte == b'\'' || byte == b'"' {
                self.quote = Some(byte);
                return Ok(());
            }
            if byte == b'>' {
                self.reset_to_text();
                emit(PullEvent::Oversized)?;
            }
            return Ok(());
        }

        if let Some(quote) = self.quote {
            self.append_bounded(byte);
            if byte == quote {
                self.quote = None;
            }
            return Ok(());
        }
        if byte == b'\'' || byte == b'"' {
            self.quote = Some(byte);
            self.append_bounded(byte);
            return Ok(());
        }
        if byte == b'>' {
            self.emit_tag(emit)?;
            self.reset_to_text();
            return Ok(());
        }

        self.append_bounded(byte);
        if self.buffer == b"!--" {
            self.buffer.clear();
            self.mode = Mode::Comment;
        } else if self.buffer == b"![CDATA[" {
            self.buffer.clear();
            self.mode = Mode::Cdata;
        }
        Ok(())
    }

    fn push_comment(&mut self, byte: u8) {
        self.push_rolling(byte, 3);
        if self.buffer.ends_with(b"-->") {
            self.reset_to_text();
        }
    }

    fn push_cdata<E>(
        &mut self,
        byte: u8,
        emit: &mut impl FnMut(PullEvent) -> Result<(), E>,
    ) -> Result<(), E> {
        if self.overflow {
            self.push_rolling(byte, 3);
            if self.buffer.ends_with(b"]]>") {
                self.reset_to_text();
                emit(PullEvent::Oversized)?;
            }
            return Ok(());
        }

        if self.buffer.len() == MAX_XML_TOKEN_BYTES {
            self.push_rolling(byte, 3);
            self.overflow = true;
        } else {
            self.buffer.push(byte);
            self.max_buffered = self.max_buffered.max(self.buffer.len());
        }
        if self.buffer.ends_with(b"]]>") {
            self.buffer.truncate(self.buffer.len() - 3);
            if self.overflow {
                emit(PullEvent::Oversized)?;
            } else {
                self.emit_raw_text(emit)?;
            }
            self.reset_to_text();
        }
        Ok(())
    }

    fn emit_tag<E>(&mut self, emit: &mut impl FnMut(PullEvent) -> Result<(), E>) -> Result<(), E> {
        let bytes = mem::take(&mut self.buffer);
        let Some(tag) = parse_tag(&bytes) else {
            return emit(PullEvent::Malformed);
        };
        match tag {
            Tag::Ignored => Ok(()),
            Tag::End(name) => emit(PullEvent::End { name }),
            Tag::Start {
                name,
                attributes,
                empty,
            } => {
                emit(PullEvent::Start {
                    name: name.clone(),
                    attributes,
                })?;
                if empty {
                    emit(PullEvent::End { name })?;
                }
                Ok(())
            }
        }
    }

    fn emit_buffered_text<E>(
        &mut self,
        emit: &mut impl FnMut(PullEvent) -> Result<(), E>,
    ) -> Result<(), E> {
        if self.overflow {
            self.buffer.clear();
            self.overflow = false;
            return emit(PullEvent::Oversized);
        }
        self.emit_raw_text(emit)
    }

    fn emit_raw_text<E>(
        &mut self,
        emit: &mut impl FnMut(PullEvent) -> Result<(), E>,
    ) -> Result<(), E> {
        if self.buffer.is_empty() {
            return Ok(());
        }
        let bytes = mem::take(&mut self.buffer);
        emit(PullEvent::Text(decode_xml(&bytes)))
    }

    fn append_bounded(&mut self, byte: u8) {
        if self.overflow {
            return;
        }
        if self.buffer.len() == MAX_XML_TOKEN_BYTES {
            self.buffer.clear();
            self.overflow = true;
            return;
        }
        self.buffer.push(byte);
        self.max_buffered = self.max_buffered.max(self.buffer.len());
    }

    fn push_rolling(&mut self, byte: u8, keep: usize) {
        if self.buffer.len() >= keep {
            let drain = self.buffer.len() + 1 - keep;
            self.buffer.drain(..drain);
        }
        self.buffer.push(byte);
    }

    fn reset_to_text(&mut self) {
        self.mode = Mode::Text;
        self.buffer.clear();
        self.quote = None;
        self.overflow = false;
    }
}

enum Tag {
    Ignored,
    End(String),
    Start {
        name: String,
        attributes: Vec<(String, String)>,
        empty: bool,
    },
}

fn parse_tag(bytes: &[u8]) -> Option<Tag> {
    let decoded = String::from_utf8_lossy(bytes);
    let trimmed = decoded.trim();
    if trimmed.starts_with('?') || trimmed.starts_with('!') {
        return Some(Tag::Ignored);
    }
    if let Some(rest) = trimmed.strip_prefix('/') {
        let name = rest.split_whitespace().next()?;
        return Some(Tag::End(local_name(name).to_owned()));
    }

    let (body, empty) = trimmed
        .strip_suffix('/')
        .map_or((trimmed, false), |body| (body.trim_end(), true));
    let name_end = body.find(char::is_whitespace).unwrap_or(body.len());
    let name = body.get(..name_end)?;
    if name.is_empty() {
        return None;
    }
    let attributes = parse_attributes(body.get(name_end..).unwrap_or_default())?;
    Some(Tag::Start {
        name: local_name(name).to_owned(),
        attributes,
        empty,
    })
}

fn parse_attributes(mut input: &str) -> Option<Vec<(String, String)>> {
    let mut attributes = Vec::new();
    loop {
        input = input.trim_start();
        if input.is_empty() {
            return Some(attributes);
        }
        let key_end = input.find(|ch: char| ch.is_whitespace() || ch == '=')?;
        let key = input.get(..key_end)?;
        input = input.get(key_end..)?.trim_start();
        input = input.strip_prefix('=')?.trim_start();
        let (value, rest) = take_attribute_value(input)?;
        attributes.push((local_name(key).to_owned(), decode_xml(value.as_bytes())));
        input = rest;
    }
}

fn take_attribute_value(input: &str) -> Option<(&str, &str)> {
    let first = input.as_bytes().first().copied()?;
    if first == b'\'' || first == b'"' {
        let quote = char::from(first);
        let tail = input.get(1..)?;
        let end = tail.find(quote)?;
        return Some((tail.get(..end)?, tail.get(end + 1..)?));
    }
    let end = input.find(char::is_whitespace).unwrap_or(input.len());
    Some((input.get(..end)?, input.get(end..)?))
}

fn local_name(name: &str) -> &str {
    name.rsplit_once(':').map_or(name, |(_, local)| local)
}

fn decode_xml(bytes: &[u8]) -> String {
    let decoded = String::from_utf8_lossy(bytes);
    let mut rest = decoded.as_ref();
    let mut output = String::with_capacity(rest.len());
    while let Some(start) = rest.find('&') {
        output.push_str(&rest[..start]);
        let entity = &rest[start + 1..];
        let Some(end) = entity.find(';').filter(|end| *end <= 12) else {
            output.push('&');
            rest = entity;
            continue;
        };
        let name = &entity[..end];
        if let Some(value) = decode_entity(name) {
            output.push(value);
        } else {
            output.push('&');
            output.push_str(name);
            output.push(';');
        }
        rest = &entity[end + 1..];
    }
    output.push_str(rest);
    output
}

fn decode_entity(name: &str) -> Option<char> {
    match name {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        value if value.starts_with("#x") => u32::from_str_radix(&value[2..], 16)
            .ok()
            .and_then(char::from_u32),
        value if value.starts_with('#') => value[1..].parse().ok().and_then(char::from_u32),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use std::convert::Infallible;

    use super::*;

    fn events(chunks: &[&[u8]]) -> Vec<PullEvent> {
        let mut parser = PullParser::default();
        let mut events = Vec::new();
        for chunk in chunks {
            parser
                .push(chunk, &mut |event| -> Result<(), Infallible> {
                    events.push(event);
                    Ok(())
                })
                .unwrap();
        }
        parser
            .finish(&mut |event| -> Result<(), Infallible> {
                events.push(event);
                Ok(())
            })
            .unwrap();
        events
    }

    #[test]
    fn tokenizes_across_every_kind_of_boundary() {
        let parsed = events(&[
            b"<tv><pro",
            b"gramme channel='bbc' start=\"x\"><title>A &amp; ",
            b"B</title><desc><![CDATA[C > D]]></desc></programme></tv>",
        ]);
        assert!(parsed.contains(&PullEvent::Text("A & B".to_owned())));
        assert!(parsed.contains(&PullEvent::Text("C > D".to_owned())));
        assert!(parsed.iter().any(|event| matches!(
            event,
            PullEvent::Start { name, attributes }
                if name == "programme" && attributes[0] == ("channel".to_owned(), "bbc".to_owned())
        )));
    }

    #[test]
    fn malformed_utf8_is_lossy_not_fatal() {
        let parsed = events(&[b"<title>bad \xff text</title>"]);
        assert!(parsed.contains(&PullEvent::Text("bad � text".to_owned())));
    }

    #[test]
    fn oversized_text_is_discarded_and_memory_is_bounded() {
        let mut parser = PullParser::default();
        let input = vec![b'x'; MAX_XML_TOKEN_BYTES + 200];
        let mut events = Vec::new();
        parser
            .push(&input, &mut |event| -> Result<(), Infallible> {
                events.push(event);
                Ok(())
            })
            .unwrap();
        parser
            .push(b"<x/>", &mut |event| -> Result<(), Infallible> {
                events.push(event);
                Ok(())
            })
            .unwrap();
        assert!(events.contains(&PullEvent::Oversized));
        assert!(parser.max_buffered_bytes() <= MAX_XML_TOKEN_BYTES);
    }

    #[test]
    fn oversized_cdata_resynchronizes_at_its_real_terminator() {
        let mut input = b"<desc><![CDATA[".to_vec();
        input.extend(vec![b'x'; MAX_XML_TOKEN_BYTES]);
        input.extend_from_slice(b"]]></desc><title>After</title>");
        let parsed = events(&[&input]);
        assert!(parsed.contains(&PullEvent::Oversized));
        assert!(parsed.contains(&PullEvent::Text("After".to_owned())));
    }
}
