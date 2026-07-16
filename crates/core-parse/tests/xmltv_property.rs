// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! XMLTV parser properties: arbitrary bytes and mutated golden fixtures never panic, chunking
//! does not change valid results, and every programme considered is emitted or skip-accounted.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::cast_possible_truncation
)]

use std::convert::Infallible;

use core_parse::{EpgWindow, ParsedProgramme, ProgrammeSink, XmltvDiagnostics, XmltvParser};
use proptest::prelude::*;

const FIXTURES: &[&[u8]] = &[
    include_bytes!("../../../fixtures/xmltv/basic.xml"),
    include_bytes!("../../../fixtures/xmltv/messy.xml"),
];

#[derive(Default)]
struct CountingSink {
    programmes: Vec<ParsedProgramme>,
}

impl ProgrammeSink for CountingSink {
    type Error = Infallible;

    fn accept(&mut self, batch: Vec<ParsedProgramme>) -> Result<(), Self::Error> {
        self.programmes.extend(batch);
        Ok(())
    }
}

fn parse_chunked(bytes: &[u8], chunk: usize) -> (Vec<ParsedProgramme>, XmltvDiagnostics) {
    let mut sink = CountingSink::default();
    let mut parser = XmltvParser::new(1_767_268_800, EpgWindow::from_hours(6, 72));
    for bytes in bytes.chunks(chunk.max(1)) {
        parser.push(bytes, &mut sink).unwrap();
    }
    let diagnostics = parser.finish(&mut sink).unwrap();
    (sink.programmes, diagnostics)
}

#[test]
fn every_chunk_size_produces_the_same_golden_result() {
    let expected = parse_chunked(FIXTURES[0], FIXTURES[0].len()).0;
    for chunk in 1..=97 {
        assert_eq!(parse_chunked(FIXTURES[0], chunk).0, expected);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn mutated_fixtures_never_panic_and_balance(
        which in 0usize..FIXTURES.len(),
        ops in proptest::collection::vec((0u8..3, any::<usize>(), any::<u8>()), 0..64),
        chunk in 1usize..97,
    ) {
        let mut bytes = FIXTURES[which].to_vec();
        for (operation, index, value) in ops {
            if bytes.is_empty() {
                bytes.push(value);
                continue;
            }
            let index = index % bytes.len();
            match operation {
                0 => bytes[index] = value,
                1 => bytes.insert(index, value),
                _ => { bytes.remove(index); }
            }
        }
        let (programmes, diagnostics) = parse_chunked(&bytes, chunk);
        prop_assert!(diagnostics.is_balanced());
        prop_assert_eq!(diagnostics.emitted(), programmes.len() as u64);
    }

    #[test]
    fn arbitrary_bytes_never_panic_and_balance(
        bytes in proptest::collection::vec(any::<u8>(), 0..8192),
        chunk in 1usize..128,
    ) {
        let (programmes, diagnostics) = parse_chunked(&bytes, chunk);
        prop_assert!(diagnostics.is_balanced());
        prop_assert_eq!(diagnostics.emitted(), programmes.len() as u64);
    }
}
