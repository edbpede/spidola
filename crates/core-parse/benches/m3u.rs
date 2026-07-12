// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Throughput benchmark for the streaming M3U parser at a 50k-channel dataset (TECH_SPEC
//! §11). The bounded-memory invariant itself is proven by the `peak_memory_is_bounded_to_
//! one_batch` unit test; this bench guards the parse cost of the 50k import budget.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::cast_possible_truncation
)]

use std::convert::Infallible;
use std::fmt::Write as _;

use core_parse::{ChannelSink, M3uParser, ParsedChannel};
use criterion::{Criterion, Throughput, criterion_group, criterion_main};

struct CountingSink {
    count: u64,
}

impl ChannelSink for CountingSink {
    type Error = Infallible;
    fn accept(&mut self, batch: Vec<ParsedChannel>) -> Result<(), Self::Error> {
        self.count += batch.len() as u64;
        Ok(())
    }
}

fn generate(channels: usize) -> Vec<u8> {
    let mut out = String::from("#EXTM3U\n");
    for i in 0..channels {
        let _ = write!(
            out,
            "#EXTINF:-1 tvg-id=\"id{i}\" tvg-logo=\"http://logo/{i}.png\" \
             group-title=\"Group {}\",Channel {i}\nhttp://host.example/live/{i}.ts\n",
            i % 64
        );
    }
    out.into_bytes()
}

fn bench_parse(c: &mut Criterion) {
    let data = generate(50_000);
    let mut group = c.benchmark_group("m3u_parse");
    group.throughput(Throughput::Bytes(data.len() as u64));
    group.bench_function("50k_channels", |b| {
        b.iter(|| {
            let mut sink = CountingSink { count: 0 };
            let mut parser = M3uParser::new();
            parser.push(&data, &mut sink).unwrap();
            parser.finish(&mut sink).unwrap();
            assert_eq!(sink.count, 50_000);
        });
    });
    group.finish();
}

criterion_group!(benches, bench_parse);
criterion_main!(benches);
