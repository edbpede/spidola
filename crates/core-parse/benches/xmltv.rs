// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Criterion throughput baseline for a generated 50k-programme XMLTV document. Criterion's
//! `--save-baseline` / `--baseline` arguments provide regression comparison support; the parser's
//! bounded-memory invariants are proven separately by unit tests.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::cast_possible_truncation
)]

use std::convert::Infallible;
use std::fmt::Write as _;
use std::hint::black_box;

use core_parse::{EpgWindow, ParsedProgramme, ProgrammeSink, XmltvParser};
use criterion::{Criterion, Throughput, criterion_group, criterion_main};

struct CountingSink {
    count: u64,
}

impl ProgrammeSink for CountingSink {
    type Error = Infallible;

    fn accept(&mut self, batch: Vec<ParsedProgramme>) -> Result<(), Self::Error> {
        self.count += batch.len() as u64;
        Ok(())
    }
}

fn generate(programmes: usize) -> String {
    let mut xml = String::with_capacity(programmes * 140);
    xml.push_str("<tv>");
    for index in 0..programmes {
        write!(
            xml,
            "<programme channel=\"c{index}\" start=\"20260101000000 +0000\" \
             stop=\"20260101010000 +0000\"><title>Programme {index}</title></programme>"
        )
        .unwrap();
    }
    xml.push_str("</tv>");
    xml
}

fn bench_xmltv(c: &mut Criterion) {
    let data = generate(50_000);
    let mut group = c.benchmark_group("xmltv_parse");
    group.throughput(Throughput::Bytes(data.len() as u64));
    group.bench_function("50k_programmes", |bencher| {
        bencher.iter(|| {
            let mut sink = CountingSink { count: 0 };
            let mut parser = XmltvParser::new(1_767_225_600, EpgWindow::from_hours(6, 72));
            for chunk in data.as_bytes().chunks(16 * 1024) {
                parser.push(black_box(chunk), &mut sink).unwrap();
            }
            let diagnostics = parser.finish(&mut sink).unwrap();
            black_box((sink.count, diagnostics));
        });
    });
    group.finish();
}

criterion_group!(benches, bench_xmltv);
criterion_main!(benches);
