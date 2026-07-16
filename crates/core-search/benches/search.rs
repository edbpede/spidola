// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Search-latency benchmark at a generated 50k-channel dataset against the sub-50 ms budget
//! (PRD §9, TECH_SPEC §11). Measures the hot prefix path through the FTS5 index.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::cast_possible_truncation
)]

use std::hint::black_box;

use core_db::{Db, NewChannel};
use core_model::channel::ChannelOverrides;
use core_model::channel::{MediaKind, channel_identity};
use core_model::ids::SourceId;
use core_model::locator::StreamLocator;
use core_model::source::{Source, SourceCommon};
use core_search::{SearchRequest, search};
use criterion::{Criterion, criterion_group, criterion_main};

const WORDS: &[&str] = &[
    "News", "Sport", "Movies", "Kids", "Music", "Docs", "Nature", "Drama", "Comedy", "Cinema",
];

fn build_db(channels: usize) -> Db {
    let db = Db::open_in_memory().unwrap();
    let source = {
        let conn = db.writer();
        core_db::repo::sources::insert(
            &conn,
            &Source::M3uFile {
                id: SourceId::new(0),
                common: SourceCommon {
                    name: "bench".to_owned(),
                    enabled: true,
                    auto_refresh_secs: None,
                },
            },
        )
        .unwrap()
    };
    let mut refresh = db.begin_staging(source).unwrap();
    let mut batch: Vec<NewChannel> = Vec::with_capacity(1000);
    for i in 0..channels {
        let name = format!("Channel {i} {}", WORDS[i % WORDS.len()]);
        let url = format!("http://host.example/live/{i}.ts");
        batch.push(NewChannel {
            identity: channel_identity(None, &url, &name),
            epg_key: None,
            name,
            group_title: Some(WORDS[i % WORDS.len()].to_owned()),
            logo: None,
            locator: StreamLocator::parse(&url).unwrap(),
            kind: MediaKind::Live,
            category: None,
            overrides: ChannelOverrides::default(),
        });
        if batch.len() == 1000 {
            refresh.stage(&batch).unwrap();
            batch.clear();
        }
    }
    if !batch.is_empty() {
        refresh.stage(&batch).unwrap();
    }
    refresh.commit(&db).unwrap();
    db
}

fn bench_search(c: &mut Criterion) {
    let db = build_db(50_000);
    let conn = db.reader().unwrap();
    c.bench_function("prefix_search_50k", |b| {
        b.iter(|| {
            let page = search(
                &conn,
                &SearchRequest {
                    text: black_box("channel 12 news"),
                    source: None,
                    kind: None,
                    offset: 0,
                    limit: 50,
                },
            )
            .unwrap();
            black_box(page);
        });
    });
}

criterion_group!(benches, bench_search);
criterion_main!(benches);
