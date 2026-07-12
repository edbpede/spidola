// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Staging-and-swap refresh: a failed refresh leaves the prior catalog intact
//! (TECH_SPEC §4.4).
//!
//! New channels stream into a per-connection **staging** table (no triggers, bounded to
//! one batch of memory at a time), then a single transaction swaps the source's live
//! `channels` rows for the staged set. Because favorites and hidden flags key on the stable
//! [`ChannelIdentity`](core_model::ids::ChannelIdentity), not the rowid, they survive the
//! swap even though every channel is renumbered.
//!
//! Correctness is proven by a fault-injection property test: injecting a failure at any
//! checkpoint (before/during staging, at the swap, before commit) rolls the whole
//! transaction back, so the prior catalog and favorites are untouched. The checkpoint hook
//! is also the batch-boundary seam that honest cancellation will use in Phase 2.

use std::sync::MutexGuard;

use rusqlite::{Connection, params};

use core_model::ids::SourceId;

use crate::error::DbResult;
use crate::pool::Db;
use crate::repo::channels::{self, IMPORT_COLUMNS, NewChannel};

/// The outcome of a completed refresh.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RefreshOutcome {
    /// Source whose catalog was replaced.
    pub source: SourceId,
    /// Number of channels in the new catalog.
    pub inserted: u64,
    /// Staged rows coalesced away by a shared `(source_id, identity)` — duplicate-identity
    /// collisions within this batch (e.g. two playlist entries carrying the same `tvg-id`).
    /// The swap keeps one row per identity; this is how many were dropped, surfaced so callers
    /// can report the loss instead of it being silent. `inserted + duplicates_dropped` equals
    /// the number of rows staged.
    pub duplicates_dropped: u64,
}

/// A point in the refresh flow at which work can be interrupted (fault injection in tests;
/// cancellation in Phase 2). Variants are constructed on the production path so the swap is
/// always checkpoint-guarded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Checkpoint {
    /// After the transaction opens, before any channel is staged.
    BeforeStage,
    /// At each `stage` batch boundary.
    DuringStage,
    /// After staging completes, before the live table is touched.
    AfterStage,
    /// After the old rows are removed, before the staged rows are inserted.
    DuringSwap,
    /// After the swap, before the final commit.
    BeforeCommit,
}

type CheckpointFn<'a> = Box<dyn Fn(Checkpoint) -> DbResult<()> + 'a>;

/// An in-progress refresh transaction. Drop rolls back if not committed.
pub struct Refresh<'a> {
    guard: MutexGuard<'a, Connection>,
    source: SourceId,
    sort_index: i64,
    inserted: u64,
    finished: bool,
    checkpoint: Option<CheckpointFn<'a>>,
}

const STAGING_DDL: &str = "\
DROP TABLE IF EXISTS _refresh_staging;
CREATE TEMP TABLE _refresh_staging (
    source_id        INTEGER,
    identity         INTEGER,
    name             TEXT,
    group_title      TEXT,
    logo             TEXT,
    locator          TEXT,
    kind             TEXT,
    category_id      INTEGER,
    user_agent       TEXT,
    headers          TEXT,
    preferred_engine TEXT,
    sort_index       INTEGER
);";

impl Db {
    /// Opens a staging-and-swap refresh for `source`.
    ///
    /// The returned [`Refresh`] holds the single writer connection (pool.rs) for its whole
    /// lifetime — from here through `commit`/drop. A caller that streams slow work in between
    /// (the import pipeline pulls an HTTP body batch-by-batch under one `Refresh`) keeps the
    /// writer for that entire span, so every other writer op (add/rename/enable/delete/favorite/
    /// setting, for any source) blocks until it finishes. That is the deliberate cost of the
    /// single-writer + one-`BEGIN IMMEDIATE` + connection-local TEMP staging model, which bounds
    /// peak memory to one batch and makes the swap atomic (§4.4). Decoupling would mean staging
    /// off the writer connection (e.g. a shared temp-file staging DB) and taking the writer only
    /// for the final swap.
    ///
    /// # Errors
    /// Returns [`DbError`](crate::error::DbError) if the transaction cannot be opened.
    pub fn begin_refresh(&self, source: SourceId) -> DbResult<Refresh<'_>> {
        self.begin_refresh_impl(source, None)
    }

    fn begin_refresh_impl<'a>(
        &'a self,
        source: SourceId,
        checkpoint: Option<CheckpointFn<'a>>,
    ) -> DbResult<Refresh<'a>> {
        let mut refresh = Refresh {
            guard: self.writer(),
            source,
            sort_index: 0,
            inserted: 0,
            finished: false,
            checkpoint,
        };
        refresh.init()?;
        Ok(refresh)
    }

    /// Test-only constructor that injects a checkpoint hook (fault injection).
    #[cfg(test)]
    pub(crate) fn begin_refresh_with<'a>(
        &'a self,
        source: SourceId,
        hook: impl Fn(Checkpoint) -> DbResult<()> + 'a,
    ) -> DbResult<Refresh<'a>> {
        self.begin_refresh_impl(source, Some(Box::new(hook)))
    }
}

impl Refresh<'_> {
    fn run_checkpoint(&self, at: Checkpoint) -> DbResult<()> {
        match &self.checkpoint {
            Some(f) => f(at),
            None => Ok(()),
        }
    }

    fn init(&mut self) -> DbResult<()> {
        self.guard.execute_batch("BEGIN IMMEDIATE")?;
        self.guard.execute_batch(STAGING_DDL)?;
        self.run_checkpoint(Checkpoint::BeforeStage)
    }

    /// Stages one batch of channels. Peak memory stays bounded to the caller's batch.
    ///
    /// # Errors
    /// Returns [`DbError`](crate::error::DbError) on a write failure or an injected fault.
    pub fn stage(&mut self, batch: &[NewChannel]) -> DbResult<()> {
        self.run_checkpoint(Checkpoint::DuringStage)?;
        let sql = format!(
            "INSERT INTO _refresh_staging({IMPORT_COLUMNS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)"
        );
        let mut stmt = self.guard.prepare_cached(&sql)?;
        for channel in batch {
            channels::insert_into(&mut stmt, self.source, channel, self.sort_index)?;
            self.sort_index += 1;
        }
        Ok(())
    }

    /// Swaps the source's live catalog for the staged set and commits.
    ///
    /// # Errors
    /// Returns [`DbError`](crate::error::DbError) on failure or an injected fault; the
    /// transaction is rolled back (prior catalog intact) before the error propagates.
    pub fn commit(mut self) -> DbResult<RefreshOutcome> {
        self.run_checkpoint(Checkpoint::AfterStage)?;
        // Count staged rows before the swap so we can report how many the `INSERT OR IGNORE`
        // below coalesces away (duplicate `(source_id, identity)` within this batch).
        let staged = self
            .guard
            .query_row("SELECT COUNT(*) FROM _refresh_staging", [], |row| {
                row.get::<_, u64>(0)
            })?;
        self.guard.execute(
            "DELETE FROM channels WHERE source_id = ?1",
            params![self.source.value()],
        )?;
        self.run_checkpoint(Checkpoint::DuringSwap)?;
        // `OR IGNORE` keeps a source with duplicate identities refreshable — dirty playlists
        // tag mirrors/variants with one `tvg-id`, which the identity design intentionally
        // treats as the same channel; a plain INSERT would abort the whole refresh and wedge
        // the source. The dropped rows are reported on the outcome rather than lost silently.
        let insert_sql = format!(
            "INSERT OR IGNORE INTO channels({IMPORT_COLUMNS}) \
             SELECT {IMPORT_COLUMNS} FROM _refresh_staging"
        );
        self.guard.execute(&insert_sql, [])?;
        let inserted = self.guard.changes();
        let duplicates_dropped = staged.saturating_sub(inserted);
        self.guard.execute("DELETE FROM _refresh_staging", [])?;
        // Merge FTS5 segments after a bulk swap so the search budget holds (PRD §9).
        crate::search_index::optimize(&self.guard)?;
        self.run_checkpoint(Checkpoint::BeforeCommit)?;
        self.guard.execute_batch("COMMIT")?;
        self.finished = true;
        self.inserted = inserted;
        Ok(RefreshOutcome {
            source: self.source,
            inserted,
            duplicates_dropped,
        })
    }
}

impl Drop for Refresh<'_> {
    fn drop(&mut self) {
        if !self.finished {
            // Best-effort rollback; there is no active transaction to save on error.
            let _ = self.guard.execute_batch("ROLLBACK");
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::error::DbError;
    use crate::repo::{channels, favorites};
    use core_model::channel::{ChannelOverrides, MediaKind, channel_identity};
    use core_model::locator::StreamLocator;
    use proptest::prelude::*;

    fn new_channel(name: &str) -> NewChannel {
        let url = format!("http://host/live/{name}");
        NewChannel {
            identity: channel_identity(None, &url, name),
            name: name.to_owned(),
            group_title: Some("News".to_owned()),
            logo: None,
            locator: StreamLocator::parse(&url).unwrap(),
            kind: MediaKind::Live,
            category: None,
            overrides: ChannelOverrides::default(),
        }
    }

    fn seed_source(db: &Db) -> SourceId {
        let conn = db.writer();
        conn.execute(
            "INSERT INTO sources(id, kind, name) VALUES (1, 'm3u-url', 'S')",
            [],
        )
        .unwrap();
        SourceId::new(1)
    }

    fn load_names(db: &Db, source: SourceId) -> Vec<String> {
        let reader = db.reader().unwrap();
        channels::list_for_source(&reader, source, 0, 10_000)
            .unwrap()
            .into_iter()
            .map(|c| c.name)
            .collect()
    }

    #[test]
    fn successful_refresh_replaces_catalog() {
        let db = Db::open_in_memory().unwrap();
        let source = seed_source(&db);
        let mut r = db.begin_refresh(source).unwrap();
        r.stage(&[new_channel("A"), new_channel("B")]).unwrap();
        let outcome = r.commit().unwrap();
        assert_eq!(outcome.inserted, 2);

        // A second refresh with a different set swaps cleanly.
        let mut r2 = db.begin_refresh(source).unwrap();
        r2.stage(&[new_channel("C")]).unwrap();
        r2.commit().unwrap();
        assert_eq!(load_names(&db, source), vec!["C".to_owned()]);
    }

    #[test]
    fn favorites_survive_refresh_via_identity() {
        let db = Db::open_in_memory().unwrap();
        let source = seed_source(&db);
        let mut r = db.begin_refresh(source).unwrap();
        r.stage(&[new_channel("A"), new_channel("B")]).unwrap();
        r.commit().unwrap();

        let fav_identity = channel_identity(None, "http://host/live/A", "A");
        {
            let conn = db.writer();
            favorites::add(&conn, source, fav_identity, 100).unwrap();
        }

        // Refresh again — "A" still present under the same identity ⇒ still a favorite.
        let mut r2 = db.begin_refresh(source).unwrap();
        r2.stage(&[new_channel("A"), new_channel("Z")]).unwrap();
        r2.commit().unwrap();
        let conn = db.reader().unwrap();
        assert!(favorites::is_favorite(&conn, source, fav_identity).unwrap());
    }

    #[test]
    fn duplicate_identities_are_counted_not_silently_dropped() {
        let db = Db::open_in_memory().unwrap();
        let source = seed_source(&db);

        // Two entries that resolve to the SAME identity (a dirty playlist tagging two mirrors
        // with one `tvg-id`). The swap keeps one row; the collision must be reported, not lost.
        let a = new_channel("A");
        let mut b = new_channel("B");
        b.identity = a.identity;

        let mut r = db.begin_refresh(source).unwrap();
        r.stage(&[a, b]).unwrap();
        let outcome = r.commit().unwrap();

        assert_eq!(outcome.inserted, 1);
        assert_eq!(outcome.duplicates_dropped, 1);
        assert_eq!(load_names(&db, source).len(), 1);
    }

    fn checkpoint_variants() -> impl Strategy<Value = Checkpoint> {
        prop_oneof![
            Just(Checkpoint::BeforeStage),
            Just(Checkpoint::DuringStage),
            Just(Checkpoint::AfterStage),
            Just(Checkpoint::DuringSwap),
            Just(Checkpoint::BeforeCommit),
        ]
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(120))]

        /// Injecting a fault at ANY checkpoint leaves the prior catalog and favorites
        /// exactly as they were — the staging-and-swap atomicity guarantee.
        #[test]
        fn fault_at_any_point_preserves_prior_catalog(
            initial in proptest::collection::vec("[a-z]{1,6}", 1..12),
            replacement in proptest::collection::vec("[a-z]{1,6}", 1..12),
            fault_at in checkpoint_variants(),
        ) {
            let db = Db::open_in_memory().unwrap();
            let source = seed_source(&db);

            // Establish a known prior catalog + a favorite on the first channel.
            let initial_channels: Vec<NewChannel> =
                initial.iter().map(|n| new_channel(n)).collect();
            let mut r = db.begin_refresh(source).unwrap();
            r.stage(&initial_channels).unwrap();
            r.commit().unwrap();
            let before = load_names(&db, source);
            let fav_identity = initial_channels[0].identity;
            {
                let conn = db.writer();
                favorites::add(&conn, source, fav_identity, 1).unwrap();
            }

            // Attempt a replacing refresh that faults at `fault_at`.
            let replacement_channels: Vec<NewChannel> =
                replacement.iter().map(|n| new_channel(n)).collect();
            let result: DbResult<RefreshOutcome> = (|| {
                let mut r = db.begin_refresh_with(source, move |cp| {
                    if cp == fault_at {
                        Err(DbError::Integrity("injected fault".to_owned()))
                    } else {
                        Ok(())
                    }
                })?;
                r.stage(&replacement_channels)?;
                r.commit()
            })();

            prop_assert!(result.is_err(), "fault at {fault_at:?} should abort the refresh");
            // Prior catalog is byte-for-byte intact.
            prop_assert_eq!(load_names(&db, source), before);
            // Favorite survived.
            let conn = db.reader().unwrap();
            prop_assert!(favorites::is_favorite(&conn, source, fav_identity).unwrap());
        }
    }
}
