// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Phase 1 exit-criteria verification (`IMPLEMENTATION_PLAN.md`).
//!
//! Drives the whole core pipeline end to end: generate a 50k-channel M3U, serve it from a
//! local HTTP stub, stream it through `core-fetch` → `core-parse` → `core-db`'s
//! staging-and-swap import, search the result under the 50 ms budget, and prove an aborted
//! refresh leaves the prior catalog (and favorites) intact. This is the orchestration glue
//! that Phase 2's `SourceService` will own; here it lives in `xtask` per the plan.

use std::fmt::Write as _;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, anyhow};
use tokio::sync::mpsc;

use core_db::{Db, repo};
use core_model::channel::channel_identity;
use core_model::ids::SourceId;
use core_model::locator::StreamLocator;
use core_model::source::{Source, SourceCommon};
use core_parse::{ChannelSink, M3uParser, ParsedChannel};
use core_search::{SearchRequest, search};

/// A summary of the verification run.
#[derive(Debug, Clone)]
pub(crate) struct Report {
    pub channels_served: usize,
    pub inserted: u64,
    pub duplicates_dropped: u64,
    pub emitted: u64,
    pub skipped: u64,
    pub invalid_locators: u64,
    pub search_hits: usize,
    pub search_min_latency: Duration,
    pub catalog_after_abort: u64,
    pub favorite_survived: bool,
}

impl std::fmt::Display for Report {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Phase 1 verification")?;
        writeln!(f, "  served channels    : {}", self.channels_served)?;
        writeln!(f, "  inserted           : {}", self.inserted)?;
        writeln!(f, "  duplicates dropped : {}", self.duplicates_dropped)?;
        writeln!(
            f,
            "  emitted / skipped  : {} / {}",
            self.emitted, self.skipped
        )?;
        writeln!(f, "  invalid locators   : {}", self.invalid_locators)?;
        writeln!(
            f,
            "  search             : {} hits, min {:?}",
            self.search_hits, self.search_min_latency
        )?;
        writeln!(
            f,
            "  after aborted refresh: {} channels, favorite kept = {}",
            self.catalog_after_abort, self.favorite_survived
        )
    }
}

const SEARCH_BUDGET: Duration = Duration::from_millis(50);

/// Runs the full Phase 1 pipeline over `channels` synthetic entries and returns a report.
///
/// # Errors
/// Propagates any failure in the fetch/parse/persist/search pipeline.
pub(crate) async fn verify(channels: usize) -> anyhow::Result<Report> {
    let playlist = generate_playlist(channels);
    let url = spawn_stub(playlist)?;

    let db = Arc::new(Db::open_in_memory().context("open in-memory db")?);
    let source = {
        let conn = db.writer();
        repo::sources::insert(
            &conn,
            &Source::M3uUrl {
                id: SourceId::new(0),
                common: SourceCommon {
                    name: "Verification source".to_owned(),
                    enabled: true,
                    auto_refresh_secs: None,
                },
                url: StreamLocator::parse(&url)?,
                user_agent: None,
                accept_invalid_tls: false,
            },
        )?
    };

    let import = import_stream(db.clone(), source, &url).await?;
    if import.inserted != channels as u64 {
        return Err(anyhow!(
            "expected {channels} channels, imported {}",
            import.inserted
        ));
    }

    // Favorite the first channel by its stable identity (survives refresh by design).
    let fav_identity = channel_identity(Some("id0"), "http://host.example/live/0.ts", "Channel 0");
    {
        let conn = db.writer();
        repo::favorites::add(&conn, source, fav_identity, 1)?;
    }

    let (search_hits, search_min_latency) = time_search(&db)?;
    if search_min_latency > SEARCH_BUDGET {
        return Err(anyhow!(
            "search min latency {search_min_latency:?} exceeds the {SEARCH_BUDGET:?} budget"
        ));
    }

    let (catalog_after_abort, favorite_survived) =
        abort_refresh_and_check(&db, source, fav_identity)?;
    if catalog_after_abort != channels as u64 {
        return Err(anyhow!(
            "aborted refresh corrupted the catalog: {catalog_after_abort} channels remain"
        ));
    }
    if !favorite_survived {
        return Err(anyhow!("favorite did not survive the aborted refresh"));
    }

    Ok(Report {
        channels_served: channels,
        inserted: import.inserted,
        duplicates_dropped: import.duplicates_dropped,
        emitted: import.emitted,
        skipped: import.skipped,
        invalid_locators: import.invalid,
        search_hits,
        search_min_latency,
        catalog_after_abort,
        favorite_survived,
    })
}

struct ImportSummary {
    inserted: u64,
    duplicates_dropped: u64,
    emitted: u64,
    skipped: u64,
    invalid: u64,
}

/// Streams the playlist from `url` into `db` via the fetch → parse → staging-and-swap path,
/// bridging the async fetch and the blocking DB work over a bounded channel.
async fn import_stream(db: Arc<Db>, source: SourceId, url: &str) -> anyhow::Result<ImportSummary> {
    let (tx, rx) = mpsc::channel::<Vec<u8>>(8);
    let worker = tokio::task::spawn_blocking(move || run_staging(&db, source, rx));

    let client = core_fetch::HttpClient::new(&core_fetch::FetchConfig::default())?;
    let mut response = client.get(&core_fetch::RequestSpec::new(url)).await?;
    while let Some(chunk) = response.chunk().await? {
        if tx.send(chunk.to_vec()).await.is_err() {
            break; // worker exited (error); its result is reported below
        }
    }
    drop(tx); // close the channel so the worker finishes

    worker.await.context("staging worker panicked")?
}

/// Blocking side: owns the parser + refresh transaction and stages received byte chunks.
fn run_staging(
    db: &Db,
    source: SourceId,
    mut rx: mpsc::Receiver<Vec<u8>>,
) -> anyhow::Result<ImportSummary> {
    let mut refresh = db.begin_refresh(source)?;
    let mut parser = M3uParser::new();
    let mut sink = StagingSink {
        refresh: &mut refresh,
        invalid: 0,
    };
    while let Some(chunk) = rx.blocking_recv() {
        parser.push(&chunk, &mut sink)?;
    }
    let diagnostics = parser.finish(&mut sink)?;
    let invalid = sink.invalid;
    // `sink`'s last use ends its `&mut refresh` borrow here, so `commit` can take it.
    let outcome = refresh.commit()?;
    Ok(ImportSummary {
        inserted: outcome.inserted,
        duplicates_dropped: outcome.duplicates_dropped,
        emitted: diagnostics.emitted(),
        skipped: diagnostics.skipped(),
        invalid,
    })
}

/// A [`ChannelSink`] that maps parsed channels to domain rows and stages them.
struct StagingSink<'a, 'db> {
    refresh: &'a mut core_db::Refresh<'db>,
    invalid: u64,
}

impl ChannelSink for StagingSink<'_, '_> {
    type Error = core_db::DbError;

    fn accept(&mut self, batch: Vec<ParsedChannel>) -> Result<(), Self::Error> {
        let mut mapped = Vec::with_capacity(batch.len());
        for parsed in batch {
            match core_api::import::map_parsed(parsed) {
                Some(channel) => mapped.push(channel),
                None => self.invalid += 1, // invalid locator: skip-and-count, never fail
            }
        }
        self.refresh.stage(&mapped)
    }
}

/// Runs a representative search five times and returns the hit count and minimum latency
/// (min-of-N is robust to scheduler noise while still gating the 50 ms budget).
fn time_search(db: &Db) -> anyhow::Result<(usize, Duration)> {
    let conn = db.reader()?;
    let request = SearchRequest {
        text: "channel 12 group",
        source: None,
        kind: None,
        offset: 0,
        limit: 50,
    };
    let mut min = Duration::MAX;
    let mut hits = 0;
    for _ in 0..5 {
        let start = Instant::now();
        let page = search(&conn, &request)?;
        min = min.min(start.elapsed());
        hits = page.channels.len();
    }
    Ok((hits, min))
}

/// Stages a different catalog then abandons the refresh without committing (an aborted
/// import) and verifies the prior catalog and favorite are intact.
fn abort_refresh_and_check(
    db: &Db,
    source: SourceId,
    favorite: core_model::ids::ChannelIdentity,
) -> anyhow::Result<(u64, bool)> {
    {
        let mut refresh = db.begin_refresh(source)?;
        let replacement = core_api::import::map_parsed(ParsedChannel {
            name: "Replacement".to_owned(),
            url: "http://host.example/live/replacement.ts".to_owned(),
            duration_secs: Some(-1.0),
            attributes: std::collections::BTreeMap::new(),
            user_agent: None,
            headers: Vec::new(),
        })
        .ok_or_else(|| anyhow!("replacement locator should be valid"))?;
        refresh.stage(&[replacement])?;
        // Drop `refresh` without commit — simulates a fault mid-import → rollback.
    }
    let conn = db.reader()?;
    let count = repo::channels::count_for_source(&conn, source)?;
    let favorite_survived = repo::favorites::is_favorite(&conn, source, favorite)?;
    Ok((count, favorite_survived))
}

fn generate_playlist(channels: usize) -> Vec<u8> {
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

/// Serves `body` once over HTTP/1.1 from a `127.0.0.1` port, returning the URL. The body is
/// streamed in chunks so the import genuinely exercises the streaming path.
fn spawn_stub(body: Vec<u8>) -> anyhow::Result<String> {
    let listener = TcpListener::bind("127.0.0.1:0").context("bind stub server")?;
    let addr = listener.local_addr().context("stub server address")?;
    thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut buf = [0_u8; 1024];
            let mut seen = Vec::new();
            while let Ok(n) = stream.read(&mut buf) {
                if n == 0 {
                    break;
                }
                seen.extend_from_slice(&buf[..n]);
                if seen.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: audio/x-mpegurl\r\n\
                 Connection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(header.as_bytes());
            for part in body.chunks(16 * 1024) {
                let _ = stream.write_all(part);
                let _ = stream.flush();
            }
        }
    });
    Ok(format!("http://{addr}/playlist.m3u"))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[tokio::test]
    async fn phase1_exit_criteria_hold_at_50k() {
        let report = verify(50_000).await.expect("phase 1 verification");
        assert_eq!(report.inserted, 50_000);
        assert_eq!(report.invalid_locators, 0);
        assert!(report.search_hits > 0);
        assert!(
            report.search_min_latency <= SEARCH_BUDGET,
            "search min {:?} exceeded budget",
            report.search_min_latency
        );
        assert_eq!(report.catalog_after_abort, 50_000);
        assert!(report.favorite_survived);
    }
}
