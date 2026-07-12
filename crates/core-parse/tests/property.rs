// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Property tests over the golden `fixtures/m3u/` corpus (TECH_SPEC §10): random mutation
//! of a real fixture, and arbitrary bytes, must never panic and must preserve the
//! skipped-entry accounting invariant (`total_seen == emitted + skipped`).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::cast_possible_truncation
)]

use std::convert::Infallible;

use core_parse::{ChannelSink, Diagnostics, M3uParser, ParsedChannel};
use proptest::prelude::*;

const FIXTURES: &[&[u8]] = &[
    include_bytes!("../../../fixtures/m3u/basic.m3u"),
    include_bytes!("../../../fixtures/m3u/attributes.m3u"),
    include_bytes!("../../../fixtures/m3u/messy.m3u"),
];

struct Counting {
    emitted: u64,
}

impl ChannelSink for Counting {
    type Error = Infallible;
    fn accept(&mut self, batch: Vec<ParsedChannel>) -> Result<(), Self::Error> {
        self.emitted += batch.len() as u64;
        Ok(())
    }
}

/// Parses `bytes` in fixed-size chunks (exercising the streaming boundary handling) and
/// returns the diagnostics plus the number of channels the sink actually received.
fn parse_chunked(bytes: &[u8], chunk: usize) -> (Diagnostics, u64) {
    let mut sink = Counting { emitted: 0 };
    let mut parser = M3uParser::new();
    for chunk in bytes.chunks(chunk.max(1)) {
        parser.push(chunk, &mut sink).unwrap();
    }
    let diag = parser.finish(&mut sink).unwrap();
    (diag, sink.emitted)
}

#[test]
fn golden_fixtures_parse_as_expected() {
    // basic.m3u: 3 channels, none skipped.
    let (diag, count) = parse_chunked(FIXTURES[0], 7);
    assert_eq!(diag.emitted(), 3);
    assert_eq!(diag.skipped(), 0);
    assert_eq!(count, 3);

    // attributes.m3u: 2 channels, none skipped.
    let (diag, _) = parse_chunked(FIXTURES[1], 13);
    assert_eq!(diag.emitted(), 2);
    assert_eq!(diag.skipped(), 0);

    // messy.m3u: 3 emitted (ok1, ok2, bare3), 2 skipped (dangling EXTINF, stray line).
    let (diag, _) = parse_chunked(FIXTURES[2], 5);
    assert_eq!(diag.emitted(), 3);
    assert_eq!(diag.skipped(), 2);
    assert!(diag.is_balanced());
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Mutating a real fixture (flip / insert / delete bytes) and re-chunking never panics
    /// and always balances the ledger.
    #[test]
    fn mutated_fixtures_never_panic(
        which in 0usize..FIXTURES.len(),
        ops in proptest::collection::vec((0u8..3, any::<usize>(), any::<u8>()), 0..64),
        chunk in 1usize..97,
    ) {
        let mut bytes = FIXTURES[which].to_vec();
        for (op, idx, val) in ops {
            if bytes.is_empty() {
                bytes.push(val);
                continue;
            }
            let i = idx % bytes.len();
            match op {
                0 => bytes[i] = val,
                1 => bytes.insert(i, val),
                _ => { bytes.remove(i); }
            }
        }
        let (diag, count) = parse_chunked(&bytes, chunk);
        prop_assert!(diag.is_balanced());
        prop_assert_eq!(diag.emitted(), count);
    }

    /// Arbitrary bytes never panic and always balance the ledger.
    #[test]
    fn arbitrary_bytes_never_panic(
        data in proptest::collection::vec(any::<u8>(), 0..4096),
        chunk in 1usize..64,
    ) {
        let (diag, count) = parse_chunked(&data, chunk);
        prop_assert!(diag.is_balanced());
        prop_assert_eq!(diag.emitted(), count);
    }
}
