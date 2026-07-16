// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Writer-free staging-and-swap refresh: a failed refresh leaves the prior catalog intact
//! (TECH_SPEC §4.4), and the single writer is held only for the final atomic swap.
//!
//! New channels stream into a **staging** SQLite database — a throwaway temp file on its own
//! connection that holds no writer lock — so an import can pull a slow HTTP body batch-by-batch
//! without blocking every other write. Peak memory stays bounded to one batch. When the download
//! finishes, [`Staging::commit`] acquires the single writer once, attaches the staged file, and
//! runs one `BEGIN IMMEDIATE` transaction that swaps the source's live `channels` rows for the
//! staged set and commits. Because favorites and hidden flags key on the stable
//! [`ChannelIdentity`](core_model::ids::ChannelIdentity), not the rowid, they survive the swap
//! even though every channel is renumbered.
//!
//! The swap's first act under the writer is a commit-time existence check: if the source was
//! deleted while the catalog staged off-lock, the transaction rolls back and commit reports
//! [`RefreshCommit::SourceRemoved`] rather than resurrecting a vanished source or tripping the
//! `channels.source_id` foreign key. This makes correctness independent of cancellation timing.
//!
//! Correctness is proven by a fault-injection property test: injecting a failure at any
//! checkpoint (before/during staging, at the swap, before commit) leaves the prior catalog and
//! favorites untouched — the off-lock staging phase never touches `main`, and a fault inside the
//! swap transaction rolls it back. The checkpoint hook is also the batch-boundary seam that
//! honest cancellation uses.

use rusqlite::{Connection, params};

use core_model::ids::SourceId;

use crate::error::{DbError, DbResult};
use crate::pool::Db;
use crate::repo::channels::{self, IMPORT_COLUMNS, NewChannel};
use crate::repo::sources;

/// The outcome of a committed refresh.
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

/// The terminal result of [`Staging::commit`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshCommit {
    /// The swap committed; the new catalog is live.
    Committed(RefreshOutcome),
    /// The source was deleted while the new catalog staged off-lock, so the swap was abandoned
    /// under the writer with nothing written — the (now-cascaded) prior state is untouched.
    /// Callers surface this as a cancellation rather than a storage error.
    SourceRemoved,
}

/// A point in the refresh flow at which work can be interrupted (fault injection in tests;
/// cancellation at batch boundaries). Variants are constructed on the production path so the
/// swap is always checkpoint-guarded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Checkpoint {
    /// After the staging database opens, before any channel is staged (off-lock).
    BeforeStage,
    /// At each `stage` batch boundary (off-lock).
    DuringStage,
    /// After the writer opens the swap transaction, before the live table is touched.
    AfterStage,
    /// After the old rows are removed, before the staged rows are inserted.
    DuringSwap,
    /// After the swap, before the final commit.
    BeforeCommit,
}

type CheckpointFn = Box<dyn Fn(Checkpoint) -> DbResult<()>>;

/// An in-progress writer-free refresh. Channels stage into a private temp-file database held by
/// `dir`; nothing touches the live catalog until [`Staging::commit`] takes the writer for the
/// swap. Dropping without committing simply discards the temp file — the prior catalog was never
/// opened for write, so there is no rollback to perform.
pub struct Staging {
    /// Dedicated connection to the temp-file staging database. Holds no writer lock.
    conn: Connection,
    /// Owns the temp directory containing the staging DB file; its `Drop` removes it. Read in
    /// [`Staging::commit`] to locate the file for `ATTACH`.
    dir: tempfile::TempDir,
    source: SourceId,
    sort_index: i64,
    checkpoint: Option<CheckpointFn>,
}

const STAGING_DDL: &str = "\
CREATE TABLE _refresh_staging (
    source_id        INTEGER,
    identity         INTEGER,
    epg_key          TEXT,
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

/// Opens the temp-file staging database, initializes it, and runs the `BeforeStage` checkpoint.
fn begin_staging_impl(source: SourceId, checkpoint: Option<CheckpointFn>) -> DbResult<Staging> {
    let dir = tempfile::TempDir::new().map_err(DbError::Staging)?;
    let conn = Connection::open(dir.path().join("staging.sqlite")).map_err(DbError::Connection)?;
    // Throwaway durability: the staging DB is discarded on commit/drop, and a crash leaves only
    // an OS-temp file (never the real catalog) for the reaper. A MEMORY journal keeps it to a
    // single file with no `-wal`/`-shm` sidecars.
    conn.execute_batch(
        "PRAGMA journal_mode = MEMORY;
         PRAGMA synchronous = OFF;",
    )?;
    conn.execute_batch(STAGING_DDL)?;
    // One transaction spans all staging inserts (bulk-insert speed on the temp file).
    conn.execute_batch("BEGIN")?;
    let staging = Staging {
        conn,
        dir,
        source,
        sort_index: 0,
        checkpoint,
    };
    staging.run_checkpoint(Checkpoint::BeforeStage)?;
    Ok(staging)
}

impl Db {
    /// Opens a writer-free staging-and-swap refresh for `source`.
    ///
    /// Staging streams into a throwaway temp-file database on its own connection, so this holds
    /// **no** writer lock: other writes (add / rename / enable / delete / favorite / setting, for
    /// any source) proceed while a slow import downloads. The single writer is taken only briefly
    /// by [`Staging::commit`], for the atomic swap. Peak memory stays bounded to one staged batch.
    ///
    /// # Errors
    /// Returns [`DbError`](crate::error::DbError) if the staging database cannot be created,
    /// opened, or initialized.
    // Kept a `Db` method for call-site symmetry with `writer`/`reader` and the prior
    // `begin_refresh`; staging holds no pool state, so `commit` re-takes the writer via its
    // `&Db` argument.
    #[allow(clippy::unused_self)]
    pub fn begin_staging(&self, source: SourceId) -> DbResult<Staging> {
        begin_staging_impl(source, None)
    }

    /// Test-only constructor that injects a checkpoint hook (fault injection).
    #[cfg(test)]
    #[allow(clippy::unused_self)]
    pub(crate) fn begin_staging_with(
        &self,
        source: SourceId,
        hook: impl Fn(Checkpoint) -> DbResult<()> + 'static,
    ) -> DbResult<Staging> {
        begin_staging_impl(source, Some(Box::new(hook)))
    }
}

impl Staging {
    fn run_checkpoint(&self, at: Checkpoint) -> DbResult<()> {
        match &self.checkpoint {
            Some(f) => f(at),
            None => Ok(()),
        }
    }

    /// Stages one batch of channels into the temp-file staging table. Peak memory stays bounded
    /// to the caller's batch, and no writer lock is held.
    ///
    /// # Errors
    /// Returns [`DbError`](crate::error::DbError) on a write failure or an injected fault.
    pub fn stage(&mut self, batch: &[NewChannel]) -> DbResult<()> {
        self.run_checkpoint(Checkpoint::DuringStage)?;
        let sql = format!(
            "INSERT INTO _refresh_staging({IMPORT_COLUMNS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)"
        );
        let mut stmt = self.conn.prepare_cached(&sql)?;
        for channel in batch {
            channels::insert_into(&mut stmt, self.source, channel, self.sort_index)?;
            self.sort_index += 1;
        }
        Ok(())
    }

    /// Finalizes the staged rows, then takes the single writer once to swap the source's live
    /// catalog for the staged set atomically (TECH_SPEC §4.4).
    ///
    /// The writer attaches the committed staging file and runs one `BEGIN IMMEDIATE` transaction:
    /// it re-checks the source still exists (its serialization point), then deletes the old rows
    /// and inserts the staged ones. A fault at any point rolls the swap back with the prior catalog
    /// intact; a source deleted mid-refresh yields [`RefreshCommit::SourceRemoved`] with nothing
    /// written. The staging database is detached on **every** exit path — the writer connection is
    /// pooled and reused, so a leaked `ATTACH` would exhaust its attach limit.
    ///
    /// # Errors
    /// Returns [`DbError`](crate::error::DbError) on a write failure or an injected fault; the swap
    /// transaction is rolled back (prior catalog intact) before the error propagates.
    pub fn commit(self, db: &Db) -> DbResult<RefreshCommit> {
        // Commit the staging transaction so its rows are durable in the temp file and visible to
        // the writer once attached. ATTACH/DETACH are illegal inside a transaction, so this must
        // straddle the writer's own BEGIN/COMMIT below.
        self.conn.execute_batch("COMMIT")?;

        let guard = db.writer();
        let staging_path = self.dir.path().join("staging.sqlite");
        let staging_path = staging_path.to_string_lossy();
        guard.execute("ATTACH DATABASE ?1 AS stg", params![staging_path.as_ref()])?;
        let result = self.swap_under_writer(&guard);
        if result.is_err() {
            // A fault left the swap transaction open; roll it back so the prior catalog survives.
            // (A clean `SourceRemoved` already rolled back inside the swap, so `is_err()` is false.)
            let _ = guard.execute_batch("ROLLBACK");
        }
        // Detach on EVERY exit path: the writer connection is long-lived and reused.
        let _ = guard.execute_batch("DETACH DATABASE stg");
        result
    }

    /// The swap itself, run under the writer with `stg` attached. On the `Ok` paths the transaction
    /// is resolved (committed, or rolled back for `SourceRemoved`); on `Err` it is left open for
    /// the caller to roll back.
    fn swap_under_writer(&self, conn: &Connection) -> DbResult<RefreshCommit> {
        conn.execute_batch("BEGIN IMMEDIATE")?;
        self.run_checkpoint(Checkpoint::AfterStage)?;
        // Serialization point: under the writer, inside the swap transaction, nothing else can
        // write. If the source vanished while we staged off-lock, abandon the swap before touching
        // the live table — this avoids resurrecting it and the foreign-key violation an insert
        // would otherwise hit.
        if !sources::exists(conn, self.source)? {
            conn.execute_batch("ROLLBACK")?;
            return Ok(RefreshCommit::SourceRemoved);
        }
        // Count staged rows before the swap so we can report how many the `INSERT OR IGNORE`
        // below coalesces away (duplicate `(source_id, identity)` within this batch).
        let staged = conn.query_row("SELECT COUNT(*) FROM stg._refresh_staging", [], |row| {
            row.get::<_, u64>(0)
        })?;
        conn.execute(
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
             SELECT {IMPORT_COLUMNS} FROM stg._refresh_staging"
        );
        conn.execute(&insert_sql, [])?;
        let inserted = conn.changes();
        let duplicates_dropped = staged.saturating_sub(inserted);
        // Merge FTS5 segments after a bulk swap so the search budget holds (PRD §9).
        crate::search_index::optimize(conn)?;
        self.run_checkpoint(Checkpoint::BeforeCommit)?;
        conn.execute_batch("COMMIT")?;
        Ok(RefreshCommit::Committed(RefreshOutcome {
            source: self.source,
            inserted,
            duplicates_dropped,
        }))
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
            epg_key: None,
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
        let mut r = db.begin_staging(source).unwrap();
        r.stage(&[new_channel("A"), new_channel("B")]).unwrap();
        let RefreshCommit::Committed(outcome) = r.commit(&db).unwrap() else {
            panic!("source exists → expected a committed swap");
        };
        assert_eq!(outcome.inserted, 2);

        // A second refresh with a different set swaps cleanly.
        let mut r2 = db.begin_staging(source).unwrap();
        r2.stage(&[new_channel("C")]).unwrap();
        r2.commit(&db).unwrap();
        assert_eq!(load_names(&db, source), vec!["C".to_owned()]);
    }

    #[test]
    fn favorites_survive_refresh_via_identity() {
        let db = Db::open_in_memory().unwrap();
        let source = seed_source(&db);
        let mut r = db.begin_staging(source).unwrap();
        r.stage(&[new_channel("A"), new_channel("B")]).unwrap();
        r.commit(&db).unwrap();

        let fav_identity = channel_identity(None, "http://host/live/A", "A");
        {
            let conn = db.writer();
            favorites::add(&conn, source, fav_identity, 100).unwrap();
        }

        // Refresh again — "A" still present under the same identity ⇒ still a favorite.
        let mut r2 = db.begin_staging(source).unwrap();
        r2.stage(&[new_channel("A"), new_channel("Z")]).unwrap();
        r2.commit(&db).unwrap();
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

        let mut r = db.begin_staging(source).unwrap();
        r.stage(&[a, b]).unwrap();
        let RefreshCommit::Committed(outcome) = r.commit(&db).unwrap() else {
            panic!("source exists → expected a committed swap");
        };

        assert_eq!(outcome.inserted, 1);
        assert_eq!(outcome.duplicates_dropped, 1);
        assert_eq!(load_names(&db, source).len(), 1);
    }

    /// A source deleted while its replacement catalog is staged off-lock commits to a clean
    /// [`RefreshCommit::SourceRemoved`] — not an error, and not a resurrected source — with the
    /// (cascaded) prior state gone.
    #[test]
    fn delete_during_staging_commits_clean_source_removed() {
        let db = Db::open_in_memory().unwrap();
        let source = seed_source(&db);

        // Prior catalog + a favorite on the first channel.
        let catalog = vec![new_channel("A"), new_channel("B")];
        let fav_identity = catalog[0].identity;
        let mut r = db.begin_staging(source).unwrap();
        r.stage(&catalog).unwrap();
        r.commit(&db).unwrap();
        {
            let conn = db.writer();
            favorites::add(&conn, source, fav_identity, 1).unwrap();
        }

        // Stage a replacement — the writer is free during staging, proving the decoupling.
        let mut s = db.begin_staging(source).unwrap();
        s.stage(&[new_channel("C"), new_channel("D")]).unwrap();
        {
            let conn = db.writer();
            sources::delete(&conn, source).unwrap(); // cascade removes catalog + favorite
        }

        // The commit-time existence check sees the source is gone and abandons the swap cleanly.
        let outcome = s.commit(&db).unwrap();
        assert!(matches!(outcome, RefreshCommit::SourceRemoved));

        // Nothing was resurrected: the source, its catalog, and its favorite are all gone.
        let reader = db.reader().unwrap();
        assert!(sources::get(&reader, source).unwrap().is_none());
        assert_eq!(channels::count_for_source(&reader, source).unwrap(), 0);
        assert!(!favorites::is_favorite(&reader, source, fav_identity).unwrap());
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

        /// Injecting a fault at ANY checkpoint leaves the prior catalog and favorites exactly as
        /// they were — the staging-and-swap atomicity guarantee across the writer-free seam. The
        /// off-lock checkpoints (before/during staging) never touch `main`; the under-writer
        /// checkpoints (after stage, during swap, before commit) roll the swap transaction back.
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
            let mut r = db.begin_staging(source).unwrap();
            r.stage(&initial_channels).unwrap();
            r.commit(&db).unwrap();
            let before = load_names(&db, source);
            let fav_identity = initial_channels[0].identity;
            {
                let conn = db.writer();
                favorites::add(&conn, source, fav_identity, 1).unwrap();
            }

            // Attempt a replacing refresh that faults at `fault_at`.
            let replacement_channels: Vec<NewChannel> =
                replacement.iter().map(|n| new_channel(n)).collect();
            let result: DbResult<RefreshCommit> = (|| {
                let mut r = db.begin_staging_with(source, move |cp| {
                    if cp == fault_at {
                        Err(DbError::Integrity("injected fault".to_owned()))
                    } else {
                        Ok(())
                    }
                })?;
                r.stage(&replacement_channels)?;
                r.commit(&db)
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
