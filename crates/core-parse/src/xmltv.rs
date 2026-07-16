// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Streaming, bounded-memory XMLTV parsing with rolling-window filtering.
//!
//! The parser owns no I/O and no clock. Callers inject `now_unix`, feed arbitrary chunks,
//! and receive bounded batches of raw programmes suitable for mapping to core domain types.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::mem;

use thiserror::Error;

use self::pull::{PullEvent, PullParser};
use self::window::EpgWindow;

pub mod pull;
pub mod window;

/// Default number of programmes handed to the sink at once.
pub const DEFAULT_PROGRAMME_BATCH_SIZE: usize = 256;
const MAX_FIELD_BYTES: usize = 64 * 1024;

/// One raw XMLTV programme, independent of `core-model` validation and persistence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedProgramme {
    /// XMLTV channel id used by the ingest layer to resolve a stable channel identity.
    pub channel: String,
    /// Programme title.
    pub title: String,
    /// Optional long description.
    pub description: Option<String>,
    /// Start time as Unix seconds.
    pub start_unix: i64,
    /// End time as Unix seconds.
    pub end_unix: i64,
}

/// Destination for bounded batches of parsed programmes.
pub trait ProgrammeSink {
    /// The sink's failure type (normally a staging-store error).
    type Error: std::error::Error + 'static;

    /// Takes ownership of one non-empty batch.
    ///
    /// # Errors
    /// Returns the sink's error to abort parsing.
    fn accept(&mut self, batch: Vec<ParsedProgramme>) -> Result<(), Self::Error>;
}

/// Why a `<programme>` element was skipped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum XmltvSkipReason {
    /// The programme had no channel id.
    MissingChannel,
    /// The programme had no title.
    MissingTitle,
    /// Start or stop time was absent.
    MissingTime,
    /// Start or stop time did not use a supported XMLTV timestamp.
    InvalidTime,
    /// The stop time did not follow the start time.
    InvalidRange,
    /// The programme did not overlap the injected retention window.
    OutsideWindow,
    /// A tag or text field exceeded the parser's fixed memory ceiling.
    OversizedToken,
    /// Markup inside the programme was malformed or truncated.
    MalformedXml,
}

/// Skip-and-count ledger for one XMLTV parse pass.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct XmltvDiagnostics {
    total_programmes: u64,
    emitted: u64,
    skipped: u64,
    malformed_tokens: u64,
    oversized_tokens: u64,
    reasons: BTreeMap<XmltvSkipReason, u64>,
}

impl XmltvDiagnostics {
    /// Number of `<programme>` elements completed or superseded.
    #[must_use]
    pub fn total_programmes(&self) -> u64 {
        self.total_programmes
    }

    /// Programmes emitted to the sink.
    #[must_use]
    pub fn emitted(&self) -> u64 {
        self.emitted
    }

    /// Programmes deliberately skipped.
    #[must_use]
    pub fn skipped(&self) -> u64 {
        self.skipped
    }

    /// Malformed pull tokens seen anywhere in the document.
    #[must_use]
    pub fn malformed_tokens(&self) -> u64 {
        self.malformed_tokens
    }

    /// Oversized pull tokens seen anywhere in the document.
    #[must_use]
    pub fn oversized_tokens(&self) -> u64 {
        self.oversized_tokens
    }

    /// Programmes skipped for one reason.
    #[must_use]
    pub fn skips_for(&self, reason: XmltvSkipReason) -> u64 {
        self.reasons.get(&reason).copied().unwrap_or(0)
    }

    /// Whether every considered programme is either emitted or skipped.
    #[must_use]
    pub fn is_balanced(&self) -> bool {
        self.total_programmes == self.emitted + self.skipped
    }

    fn record_emitted(&mut self) {
        self.total_programmes += 1;
        self.emitted += 1;
    }

    fn record_skip(&mut self, reason: XmltvSkipReason) {
        self.total_programmes += 1;
        self.skipped += 1;
        *self.reasons.entry(reason).or_insert(0) += 1;
    }
}

/// XMLTV pipeline failure. Malformed input is skip-and-count; only the sink aborts a pass.
#[derive(Debug, Error)]
pub enum XmltvParseError<E: std::error::Error + 'static> {
    /// The programme sink rejected a batch.
    #[error("the programme sink rejected a batch")]
    Sink(#[source] E),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    Title,
    Description,
}

#[derive(Debug)]
struct Capture {
    field: Field,
    text: String,
}

#[derive(Debug)]
struct PendingProgramme {
    channel: Option<String>,
    start: Option<String>,
    stop: Option<String>,
    title: Option<String>,
    description: Option<String>,
    invalid: Option<XmltvSkipReason>,
}

/// Incremental XMLTV parser. Feed chunks with [`Self::push`], then call [`Self::finish`].
#[derive(Debug)]
pub struct XmltvParser {
    now_unix: i64,
    window: EpgWindow,
    batch_size: usize,
    batch: Vec<ParsedProgramme>,
    diagnostics: XmltvDiagnostics,
    pull: PullParser,
    current: Option<PendingProgramme>,
    capture: Option<Capture>,
    max_batch: usize,
    max_pull_buffer: usize,
}

impl XmltvParser {
    /// Creates a parser using the default batch size.
    #[must_use]
    pub fn new(now_unix: i64, window: EpgWindow) -> Self {
        Self::with_batch_size(now_unix, window, DEFAULT_PROGRAMME_BATCH_SIZE)
    }

    /// Creates a parser with an explicit batch size. Zero is normalized to one.
    #[must_use]
    pub fn with_batch_size(now_unix: i64, window: EpgWindow, batch_size: usize) -> Self {
        let batch_size = batch_size.max(1);
        Self {
            now_unix,
            window,
            batch_size,
            batch: Vec::with_capacity(batch_size),
            diagnostics: XmltvDiagnostics::default(),
            pull: PullParser::default(),
            current: None,
            capture: None,
            max_batch: 0,
            max_pull_buffer: 0,
        }
    }

    /// Feeds an arbitrary byte chunk.
    ///
    /// # Errors
    /// Returns [`XmltvParseError::Sink`] if a full batch is rejected.
    pub fn push<S: ProgrammeSink>(
        &mut self,
        bytes: &[u8],
        sink: &mut S,
    ) -> Result<(), XmltvParseError<S::Error>> {
        let mut pull = mem::take(&mut self.pull);
        let result = pull.push(bytes, &mut |event| self.handle(event, sink));
        self.max_pull_buffer = self.max_pull_buffer.max(pull.max_buffered_bytes());
        self.pull = pull;
        result
    }

    /// Completes the document, flushes the final batch, and returns diagnostics.
    ///
    /// # Errors
    /// Returns [`XmltvParseError::Sink`] if the final batch is rejected.
    pub fn finish<S: ProgrammeSink>(
        mut self,
        sink: &mut S,
    ) -> Result<XmltvDiagnostics, XmltvParseError<S::Error>> {
        let pull = mem::take(&mut self.pull);
        pull.finish(&mut |event| self.handle(event, sink))?;
        if self.current.is_some() {
            self.skip_current(XmltvSkipReason::MalformedXml);
        }
        self.flush(sink)?;
        Ok(self.diagnostics)
    }

    /// Largest batch accumulated during this pass.
    #[must_use]
    pub fn max_batch_len(&self) -> usize {
        self.max_batch
    }

    /// Largest pull-token buffer accumulated during this pass.
    #[must_use]
    pub fn max_pull_buffered_bytes(&self) -> usize {
        self.max_pull_buffer.max(self.pull.max_buffered_bytes())
    }

    fn handle<S: ProgrammeSink>(
        &mut self,
        event: PullEvent,
        sink: &mut S,
    ) -> Result<(), XmltvParseError<S::Error>> {
        match event {
            PullEvent::Start { name, attributes } => self.start(&name, &attributes),
            PullEvent::End { name } => self.end(&name, sink)?,
            PullEvent::Text(text) => self.text(&text),
            PullEvent::Malformed => {
                self.diagnostics.malformed_tokens += 1;
                self.invalidate(XmltvSkipReason::MalformedXml);
            }
            PullEvent::Oversized => {
                self.diagnostics.oversized_tokens += 1;
                self.invalidate(XmltvSkipReason::OversizedToken);
            }
        }
        Ok(())
    }

    fn start(&mut self, name: &str, attributes: &[(String, String)]) {
        match name {
            "programme" => {
                if self.current.is_some() {
                    self.skip_current(XmltvSkipReason::MalformedXml);
                }
                self.capture = None;
                self.current = Some(PendingProgramme {
                    channel: attribute(attributes, "channel").map(str::to_owned),
                    start: attribute(attributes, "start").map(str::to_owned),
                    stop: attribute(attributes, "stop").map(str::to_owned),
                    title: None,
                    description: None,
                    invalid: None,
                });
            }
            "title" if self.current.is_some() => self.begin_capture(Field::Title),
            "desc" if self.current.is_some() => self.begin_capture(Field::Description),
            _ => {}
        }
    }

    fn end<S: ProgrammeSink>(
        &mut self,
        name: &str,
        sink: &mut S,
    ) -> Result<(), XmltvParseError<S::Error>> {
        match name {
            "title" => self.commit_capture(Field::Title),
            "desc" => self.commit_capture(Field::Description),
            "programme" => {
                self.capture = None;
                self.complete_current(sink)?;
            }
            _ => {}
        }
        Ok(())
    }

    fn begin_capture(&mut self, field: Field) {
        self.capture = Some(Capture {
            field,
            text: String::new(),
        });
    }

    fn text(&mut self, text: &str) {
        let Some(capture) = self.capture.as_mut() else {
            return;
        };
        if capture.text.len().saturating_add(text.len()) > MAX_FIELD_BYTES {
            self.capture = None;
            self.invalidate(XmltvSkipReason::OversizedToken);
            self.diagnostics.oversized_tokens += 1;
            return;
        }
        capture.text.push_str(text);
    }

    fn commit_capture(&mut self, expected: Field) {
        let Some(capture) = self
            .capture
            .take()
            .filter(|capture| capture.field == expected)
        else {
            return;
        };
        let value = capture.text.trim().to_owned();
        let Some(current) = self.current.as_mut() else {
            return;
        };
        match expected {
            Field::Title if current.title.is_none() => current.title = Some(value),
            Field::Description if current.description.is_none() && !value.is_empty() => {
                current.description = Some(value);
            }
            Field::Title | Field::Description => {}
        }
    }

    fn invalidate(&mut self, reason: XmltvSkipReason) {
        if let Some(current) = self.current.as_mut()
            && current.invalid.is_none()
        {
            current.invalid = Some(reason);
        }
    }

    fn complete_current<S: ProgrammeSink>(
        &mut self,
        sink: &mut S,
    ) -> Result<(), XmltvParseError<S::Error>> {
        let Some(current) = self.current.take() else {
            return Ok(());
        };
        let programme = match self.validate(current) {
            Ok(programme) => programme,
            Err(reason) => {
                self.diagnostics.record_skip(reason);
                return Ok(());
            }
        };
        self.batch.push(programme);
        self.max_batch = self.max_batch.max(self.batch.len());
        self.diagnostics.record_emitted();
        if self.batch.len() >= self.batch_size {
            self.flush(sink)?;
        }
        Ok(())
    }

    fn validate(&self, current: PendingProgramme) -> Result<ParsedProgramme, XmltvSkipReason> {
        if let Some(reason) = current.invalid {
            return Err(reason);
        }
        let channel = non_empty(current.channel).ok_or(XmltvSkipReason::MissingChannel)?;
        let title = non_empty(current.title).ok_or(XmltvSkipReason::MissingTitle)?;
        let start = current.start.ok_or(XmltvSkipReason::MissingTime)?;
        let stop = current.stop.ok_or(XmltvSkipReason::MissingTime)?;
        let start_unix = parse_xmltv_timestamp(&start).ok_or(XmltvSkipReason::InvalidTime)?;
        let end_unix = parse_xmltv_timestamp(&stop).ok_or(XmltvSkipReason::InvalidTime)?;
        if end_unix <= start_unix {
            return Err(XmltvSkipReason::InvalidRange);
        }
        if !self.window.includes(self.now_unix, start_unix, end_unix) {
            return Err(XmltvSkipReason::OutsideWindow);
        }
        Ok(ParsedProgramme {
            channel,
            title,
            description: current.description,
            start_unix,
            end_unix,
        })
    }

    fn skip_current(&mut self, reason: XmltvSkipReason) {
        self.current = None;
        self.capture = None;
        self.diagnostics.record_skip(reason);
    }

    fn flush<S: ProgrammeSink>(&mut self, sink: &mut S) -> Result<(), XmltvParseError<S::Error>> {
        if self.batch.is_empty() {
            return Ok(());
        }
        sink.accept(mem::take(&mut self.batch))
            .map_err(XmltvParseError::Sink)
    }
}

fn attribute<'a>(attributes: &'a [(String, String)], key: &str) -> Option<&'a str> {
    attributes
        .iter()
        .find_map(|(name, value)| (name == key).then_some(value.as_str()))
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.trim().is_empty())
}

/// Parses the XMLTV `YYYYMMDDhhmmss ±HHMM` family into Unix seconds.
///
/// Seconds and lower-order time fields may be omitted as XMLTV permits; missing fields and a
/// missing zone use zero/UTC, which keeps parsing deterministic rather than consulting a host
/// locale.
#[must_use]
pub fn parse_xmltv_timestamp(input: &str) -> Option<i64> {
    let mut parts = input.split_whitespace();
    let digits = parts.next()?;
    let zone = parts.next();
    if parts.next().is_some() || !matches!(digits.len(), 8 | 10 | 12 | 14) {
        return None;
    }
    let year = parse_digits(digits, 0, 4)?;
    let month = parse_digits(digits, 4, 6)?;
    let day = parse_digits(digits, 6, 8)?;
    let hour = parse_optional(digits, 8, 10)?;
    let minute = parse_optional(digits, 10, 12)?;
    let second = parse_optional(digits, 12, 14)?;
    if !valid_date(year, month, day) || hour > 23 || minute > 59 || second > 60 {
        return None;
    }
    let days = days_from_civil(i64::from(year), i64::from(month), i64::from(day));
    let local = days
        .checked_mul(86_400)?
        .checked_add(i64::from(hour) * 3_600)?
        .checked_add(i64::from(minute) * 60)?
        .checked_add(i64::from(second.min(59)))?;
    local.checked_sub(parse_zone(zone.unwrap_or("+0000"))?)
}

fn parse_optional(input: &str, start: usize, end: usize) -> Option<u32> {
    if input.len() < end {
        Some(0)
    } else {
        parse_digits(input, start, end)
    }
}

fn parse_digits(input: &str, start: usize, end: usize) -> Option<u32> {
    input.get(start..end)?.parse().ok()
}

fn parse_zone(zone: &str) -> Option<i64> {
    if zone == "Z" {
        return Some(0);
    }
    let sign = match zone.as_bytes().first().copied()? {
        b'+' => 1_i64,
        b'-' => -1_i64,
        _ => return None,
    };
    if zone.len() != 5 {
        return None;
    }
    let hours = i64::from(parse_digits(zone, 1, 3)?);
    let minutes = i64::from(parse_digits(zone, 3, 5)?);
    (hours <= 23 && minutes <= 59).then_some(sign * (hours * 3_600 + minutes * 60))
}

fn valid_date(year: u32, month: u32, day: u32) -> bool {
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    (1..=max_day).contains(&day)
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let adjusted_year = year - i64::from(month <= 2);
    let era = adjusted_year.div_euclid(400);
    let year_of_era = adjusted_year - era * 400;
    let shifted_month = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * shifted_month + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use std::convert::Infallible;
    use std::fmt::Write as _;

    use super::*;

    #[derive(Default)]
    struct Collector {
        programmes: Vec<ParsedProgramme>,
        max_batch: usize,
    }

    impl ProgrammeSink for Collector {
        type Error = Infallible;

        fn accept(&mut self, batch: Vec<ParsedProgramme>) -> Result<(), Self::Error> {
            self.max_batch = self.max_batch.max(batch.len());
            self.programmes.extend(batch);
            Ok(())
        }
    }

    fn parse(input: &[u8], chunk: usize, batch: usize) -> (Collector, XmltvDiagnostics) {
        let mut sink = Collector::default();
        let mut parser =
            XmltvParser::with_batch_size(1_767_268_800, EpgWindow::from_hours(6, 72), batch);
        for bytes in input.chunks(chunk) {
            parser.push(bytes, &mut sink).unwrap();
        }
        let diagnostics = parser.finish(&mut sink).unwrap();
        (sink, diagnostics)
    }

    #[test]
    fn parses_and_filters_a_document_in_tiny_chunks() {
        let input = include_bytes!("../../../fixtures/xmltv/basic.xml");
        let (sink, diagnostics) = parse(input, 3, 1);
        assert_eq!(sink.programmes.len(), 2);
        assert_eq!(sink.max_batch, 1);
        assert_eq!(sink.programmes[0].channel, "bbc.one");
        assert_eq!(sink.programmes[0].title, "Morning & News");
        assert_eq!(
            sink.programmes[0].description.as_deref(),
            Some("Headlines > weather")
        );
        assert_eq!(diagnostics.emitted(), 2);
        assert_eq!(diagnostics.skips_for(XmltvSkipReason::OutsideWindow), 1);
        assert!(diagnostics.is_balanced());
    }

    #[test]
    fn malformed_and_missing_fields_are_accounted() {
        let input = include_bytes!("../../../fixtures/xmltv/messy.xml");
        let (sink, diagnostics) = parse(input, 17, 100);
        assert_eq!(sink.programmes.len(), 1);
        assert_eq!(diagnostics.skips_for(XmltvSkipReason::MissingTitle), 1);
        assert_eq!(diagnostics.skips_for(XmltvSkipReason::InvalidTime), 1);
        assert!(diagnostics.is_balanced());
    }

    #[test]
    fn timestamps_honor_offsets_and_calendar_rules() {
        assert_eq!(parse_xmltv_timestamp("19700101010000 +0100"), Some(0));
        assert_eq!(parse_xmltv_timestamp("19700101000000 Z"), Some(0));
        assert!(parse_xmltv_timestamp("20260230000000 +0000").is_none());
        assert!(parse_xmltv_timestamp("20260101240000 +0000").is_none());
    }

    #[test]
    fn batch_and_token_buffers_stay_bounded() {
        let mut input = String::from("<tv>");
        for index in 0..500 {
            write!(
                input,
                "<programme channel=\"c{index}\" start=\"20260101000000 +0000\" \
                 stop=\"20260101010000 +0000\"><title>T</title></programme>"
            )
            .unwrap();
        }
        input.push_str("</tv>");
        let mut sink = Collector::default();
        let mut parser = XmltvParser::with_batch_size(1_767_225_600, EpgWindow::default(), 13);
        parser.push(input.as_bytes(), &mut sink).unwrap();
        assert!(parser.max_batch_len() <= 13);
        assert!(parser.max_pull_buffered_bytes() <= pull::MAX_XML_TOKEN_BYTES);
        let diagnostics = parser.finish(&mut sink).unwrap();
        assert_eq!(diagnostics.emitted(), 500);
        assert!(sink.max_batch <= 13);
    }

    #[test]
    fn malformed_utf8_and_an_oversized_field_do_not_abort_following_programmes() {
        let mut input = b"<tv><programme channel='bad' start='20260101120000 +0000' \
            stop='20260101130000 +0000'><title>"
            .to_vec();
        input.extend(vec![0xff; MAX_FIELD_BYTES + 20]);
        input.extend_from_slice(
            b"</title></programme><programme channel='ok' start='20260101120000 +0000' \
              stop='20260101130000 +0000'><title>After</title></programme></tv>",
        );
        let (sink, diagnostics) = parse(&input, 29, 20);
        assert_eq!(sink.programmes.len(), 1);
        assert_eq!(sink.programmes[0].title, "After");
        assert_eq!(diagnostics.skips_for(XmltvSkipReason::OversizedToken), 1);
        assert!(diagnostics.oversized_tokens() >= 1);
    }
}
