// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Bounded EPG persistence and now/next queries (PRD §6.6).

use rusqlite::{Connection, Row, params, params_from_iter, types::Value};

use core_model::{ChannelIdentity, EpgEntry, EpgEntryId, SecretRef, SourceId};

use crate::error::{DbError, DbResult};
use crate::pool::Db;
use crate::repo::sources;

/// Terminal outcome of an atomic EPG staging swap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EpgCommit {
    /// The source still existed and its schedule was replaced.
    Committed { inserted: u64 },
    /// The source was removed while the schedule staged off-lock.
    SourceRemoved,
    /// A newer refresh superseded this one (or it was cancelled) before the swap; the live
    /// schedule was left untouched so a slow older refresh cannot overwrite a newer guide.
    Superseded,
}

/// One channel's current and next guide entries from a batched lookup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EpgNowNext {
    pub channel: ChannelIdentity,
    pub current: Option<EpgEntry>,
    pub next: Option<EpgEntry>,
}

/// Writer-free, bounded EPG staging store. Dropping it leaves the live schedule untouched.
pub struct EpgStaging {
    conn: Connection,
    dir: tempfile::TempDir,
    source: SourceId,
}

const STAGING_DDL: &str = "\
CREATE TABLE _epg_staging (
    channel_identity INTEGER NOT NULL,
    title            TEXT NOT NULL,
    description      TEXT,
    start_unix       INTEGER NOT NULL,
    end_unix         INTEGER NOT NULL,
    CHECK(end_unix > start_unix),
    UNIQUE(channel_identity, start_unix, end_unix)
);";

impl Db {
    /// Opens a private temp-file EPG staging store without taking the live writer.
    ///
    /// # Errors
    /// Returns [`DbError`] when the temporary database cannot be created.
    #[allow(clippy::unused_self)]
    pub fn begin_epg_staging(&self, source: SourceId) -> DbResult<EpgStaging> {
        let dir = tempfile::TempDir::new().map_err(DbError::Staging)?;
        let conn =
            Connection::open(dir.path().join("epg-staging.sqlite")).map_err(DbError::Connection)?;
        conn.execute_batch(
            "PRAGMA journal_mode = MEMORY;
             PRAGMA synchronous = OFF;",
        )?;
        conn.execute_batch(STAGING_DDL)?;
        conn.execute_batch("BEGIN")?;
        Ok(EpgStaging { conn, dir, source })
    }
}

impl EpgStaging {
    /// Writes one parser-sized batch to the private staging file.
    ///
    /// # Errors
    /// Returns [`DbError`] if the batch cannot be staged.
    pub fn stage(&mut self, entries: &[EpgEntry]) -> DbResult<()> {
        let mut statement = self.conn.prepare_cached(
            "INSERT INTO _epg_staging(\
             channel_identity, title, description, start_unix, end_unix) \
             VALUES (?1, ?2, ?3, ?4, ?5) \
             ON CONFLICT(channel_identity, start_unix, end_unix) DO UPDATE SET \
             title = excluded.title, description = excluded.description",
        )?;
        for entry in entries
            .iter()
            .filter(|entry| entry.source_id == self.source)
        {
            statement.execute(params![
                entry.channel.to_storage(),
                entry.title,
                entry.description,
                entry.start_unix,
                entry.end_unix,
            ])?;
        }
        Ok(())
    }

    /// Atomically swaps the staged schedule into the live database.
    ///
    /// `is_cancelled` is polled **under the live writer lock**, inside the `BEGIN IMMEDIATE`
    /// transaction that serializes this swap against every other commit — once on entry (a refresh
    /// already superseded before it won the writer lock abandons the swap, so it can never overwrite
    /// the newer guide a superseding refresh already committed) and once more immediately before the
    /// commit (a supersession that lands while the swap is being built still abandons it). Only a
    /// cancellation arriving within the `COMMIT` itself is not honoured — cancellation never
    /// hard-aborts a task mid-DB-write.
    ///
    /// # Errors
    /// Returns [`DbError`] if the staging transaction or live swap fails.
    pub fn commit(self, db: &Db, is_cancelled: &dyn Fn() -> bool) -> DbResult<EpgCommit> {
        self.conn.execute_batch("COMMIT")?;
        let guard = db.writer();
        let path = self.dir.path().join("epg-staging.sqlite");
        let path = path.to_string_lossy();
        guard.execute("ATTACH DATABASE ?1 AS epg_stg", params![path.as_ref()])?;
        let result = self.swap_under_writer(&guard, is_cancelled);
        if result.is_err() {
            let _ = guard.execute_batch("ROLLBACK");
        }
        let _ = guard.execute_batch("DETACH DATABASE epg_stg");
        result
    }

    fn swap_under_writer(
        &self,
        conn: &Connection,
        is_cancelled: &dyn Fn() -> bool,
    ) -> DbResult<EpgCommit> {
        conn.execute_batch("BEGIN IMMEDIATE")?;
        // First cancellation check, now that the writer lock serializes this swap against every
        // other commit: a supersession cancels the older refresh before the newer one commits, so
        // an older task that reaches this locked section already superseded abandons the swap
        // rather than clobbering the newer guide.
        if is_cancelled() {
            conn.execute_batch("ROLLBACK")?;
            return Ok(EpgCommit::Superseded);
        }
        if !sources::exists(conn, self.source)? {
            conn.execute_batch("ROLLBACK")?;
            return Ok(EpgCommit::SourceRemoved);
        }
        conn.execute(
            "DELETE FROM epg_entries WHERE source_id = ?1",
            params![self.source.value()],
        )?;
        conn.execute(
            "INSERT INTO epg_entries(\
             source_id, channel_identity, title, description, start_unix, end_unix) \
             SELECT ?1, channel_identity, title, description, start_unix, end_unix \
             FROM epg_stg._epg_staging",
            params![self.source.value()],
        )?;
        let inserted = conn.changes();
        // Second cancellation check, immediately before the commit: registration takes the registry
        // lock, not this writer lock, so a supersession can land *during* the DELETE/INSERT above.
        // Re-checking here abandons the just-built swap so a refresh cancelled mid-swap does not
        // install its now-superseded schedule. (A cancellation landing within the COMMIT itself is
        // not honoured — cancellation never hard-aborts a task mid-DB-write, per the events module.)
        if is_cancelled() {
            conn.execute_batch("ROLLBACK")?;
            return Ok(EpgCommit::Superseded);
        }
        conn.execute_batch("COMMIT")?;
        Ok(EpgCommit::Committed { inserted })
    }
}

/// Stores the opaque secure-store reference for a source's XMLTV feed.
///
/// # Errors
/// Returns [`DbError`] on a write failure.
pub fn set_feed(conn: &Connection, source: SourceId, secret_ref: &SecretRef) -> DbResult<()> {
    conn.execute(
        "INSERT INTO epg_feeds(source_id, secret_ref) VALUES (?1, ?2) \
         ON CONFLICT(source_id) DO UPDATE SET secret_ref = excluded.secret_ref",
        params![source.value(), secret_ref.as_str()],
    )?;
    Ok(())
}

/// Returns the opaque secure-store reference for a source's XMLTV feed.
///
/// # Errors
/// Returns [`DbError`] on a query failure.
pub fn get_feed(conn: &Connection, source: SourceId) -> DbResult<Option<SecretRef>> {
    let mut statement = conn.prepare("SELECT secret_ref FROM epg_feeds WHERE source_id = ?1")?;
    let mut rows = statement.query(params![source.value()])?;
    Ok(rows
        .next()?
        .map(|row| row.get::<_, String>(0).map(SecretRef::new))
        .transpose()?)
}

/// Removes a source's configured feed, returning its secure-store reference for cleanup.
///
/// # Errors
/// Returns [`DbError`] on a query or write failure.
pub fn remove_feed(conn: &Connection, source: SourceId) -> DbResult<Option<SecretRef>> {
    let feed = get_feed(conn, source)?;
    conn.execute(
        "DELETE FROM epg_feeds WHERE source_id = ?1",
        params![source.value()],
    )?;
    Ok(feed)
}

/// Replaces one source's rolling EPG window atomically.
///
/// # Errors
/// Returns [`DbError`](crate::error::DbError) on a query failure.
pub fn replace_window(
    conn: &mut Connection,
    source: SourceId,
    earliest_unix: i64,
    latest_unix: i64,
    entries: &[EpgEntry],
) -> DbResult<u64> {
    let transaction = conn.transaction()?;
    transaction.execute(
        "DELETE FROM epg_entries WHERE source_id = ?1",
        params![source.value()],
    )?;
    let mut insert = transaction.prepare(
        "INSERT INTO epg_entries(\
         source_id, channel_identity, title, description, start_unix, end_unix) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
         ON CONFLICT(source_id, channel_identity, start_unix, end_unix) DO UPDATE SET \
         title = excluded.title, description = excluded.description",
    )?;
    let mut written = 0_u64;
    for entry in entries.iter().filter(|entry| {
        entry.source_id == source
            && entry.end_unix > earliest_unix
            && entry.start_unix < latest_unix
            && entry.end_unix > entry.start_unix
    }) {
        insert.execute(params![
            source.value(),
            entry.channel.to_storage(),
            entry.title,
            entry.description,
            entry.start_unix,
            entry.end_unix,
        ])?;
        written = written.saturating_add(1);
    }
    drop(insert);
    transaction.commit()?;
    Ok(written)
}

/// Removes entries wholly outside the configured rolling window.
///
/// # Errors
/// Returns [`DbError`](crate::error::DbError) on a write failure.
pub fn prune(conn: &Connection, earliest_unix: i64, latest_unix: i64) -> DbResult<u64> {
    let count = conn.execute(
        "DELETE FROM epg_entries WHERE end_unix <= ?1 OR start_unix >= ?2",
        params![earliest_unix, latest_unix],
    )?;
    Ok(u64::try_from(count).unwrap_or(u64::MAX))
}

/// Returns the current and next programme for one channel.
///
/// # Errors
/// Returns [`DbError`](crate::error::DbError) on a query failure.
pub fn now_next(
    conn: &Connection,
    source: SourceId,
    channel: ChannelIdentity,
    now_unix: i64,
) -> DbResult<(Option<EpgEntry>, Option<EpgEntry>)> {
    let mut statement = conn.prepare(
        "SELECT id, source_id, channel_identity, title, description, start_unix, end_unix \
         FROM epg_entries \
         WHERE source_id = ?1 AND channel_identity = ?2 AND end_unix > ?3 \
         ORDER BY start_unix LIMIT 2",
    )?;
    let mut rows = statement.query(params![source.value(), channel.to_storage(), now_unix])?;
    let first = rows.next()?.map(map_entry).transpose()?;
    let second = rows.next()?.map(map_entry).transpose()?;
    Ok(classify_now_next(first, second, now_unix))
}

/// Returns current and next programmes for a bounded channel selection in one query.
/// Results preserve the input order, including missing and repeated identities.
///
/// # Errors
/// Returns [`DbError`](crate::error::DbError) on a query failure.
pub fn now_next_batch(
    conn: &Connection,
    source: SourceId,
    channels: &[ChannelIdentity],
    now_unix: i64,
) -> DbResult<Vec<EpgNowNext>> {
    if channels.is_empty() {
        return Ok(Vec::new());
    }

    let requested = channels
        .iter()
        .enumerate()
        .map(|(ordinal, _)| format!("(?{}, {ordinal})", ordinal + 3))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "WITH requested(channel_identity, ordinal) AS (VALUES {requested}), \
         upcoming AS (\
           SELECT requested.ordinal, \
                  epg_entries.id, epg_entries.source_id, epg_entries.channel_identity, \
                  epg_entries.title, epg_entries.description, \
                  epg_entries.start_unix, epg_entries.end_unix, \
                  row_number() OVER (\
                    PARTITION BY requested.ordinal ORDER BY epg_entries.start_unix\
                  ) AS sequence \
           FROM requested \
           JOIN epg_entries \
             ON epg_entries.channel_identity = requested.channel_identity \
           WHERE epg_entries.source_id = ?1 AND epg_entries.end_unix > ?2\
         ) \
         SELECT ordinal, id, source_id, channel_identity, title, description, \
                start_unix, end_unix \
         FROM upcoming WHERE sequence <= 2 ORDER BY ordinal, sequence"
    );
    let mut values = Vec::with_capacity(channels.len() + 2);
    values.push(Value::Integer(source.value()));
    values.push(Value::Integer(now_unix));
    values.extend(
        channels
            .iter()
            .map(|identity| Value::Integer(identity.to_storage())),
    );

    let mut statement = conn.prepare(&sql)?;
    let rows = statement.query_map(params_from_iter(values), |row| {
        Ok((row.get::<_, usize>("ordinal")?, map_entry(row)?))
    })?;
    let mut upcoming = vec![(None, None); channels.len()];
    for row in rows {
        let (ordinal, entry) = row?;
        let (first, second) = &mut upcoming[ordinal];
        if first.is_none() {
            *first = Some(entry);
        } else if second.is_none() {
            *second = Some(entry);
        }
    }

    Ok(channels
        .iter()
        .copied()
        .zip(upcoming)
        .map(|(channel, (first, second))| {
            let (current, next) = classify_now_next(first, second, now_unix);
            EpgNowNext {
                channel,
                current,
                next,
            }
        })
        .collect())
}

fn classify_now_next(
    first: Option<EpgEntry>,
    second: Option<EpgEntry>,
    now_unix: i64,
) -> (Option<EpgEntry>, Option<EpgEntry>) {
    match first {
        Some(entry) if entry.is_current(now_unix) => (Some(entry), second),
        Some(entry) => (None, Some(entry)),
        None => (None, None),
    }
}

/// Lists a bounded page intersecting a time window.
///
/// # Errors
/// Returns [`DbError`](crate::error::DbError) on a query failure.
pub fn list_window(
    conn: &Connection,
    source: SourceId,
    channel: ChannelIdentity,
    earliest_unix: i64,
    latest_unix: i64,
    offset: u32,
    limit: u32,
) -> DbResult<Vec<EpgEntry>> {
    let mut statement = conn.prepare(
        "SELECT id, source_id, channel_identity, title, description, start_unix, end_unix \
         FROM epg_entries WHERE source_id = ?1 AND channel_identity = ?2 \
         AND end_unix > ?3 AND start_unix < ?4 \
         ORDER BY start_unix LIMIT ?5 OFFSET ?6",
    )?;
    let rows = statement.query_map(
        params![
            source.value(),
            channel.to_storage(),
            earliest_unix,
            latest_unix,
            limit,
            offset
        ],
        map_entry,
    )?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

fn map_entry(row: &Row<'_>) -> rusqlite::Result<EpgEntry> {
    Ok(EpgEntry {
        id: EpgEntryId::new(row.get("id")?),
        source_id: SourceId::new(row.get("source_id")?),
        channel: ChannelIdentity::from_storage(row.get("channel_identity")?),
        title: row.get("title")?,
        description: row.get("description")?,
        start_unix: row.get("start_unix")?,
        end_unix: row.get("end_unix")?,
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::pool::Db;

    fn entry(start: i64, end: i64, title: &str) -> EpgEntry {
        entry_for(7, start, end, title)
    }

    fn entry_for(channel: u64, start: i64, end: i64, title: &str) -> EpgEntry {
        EpgEntry {
            id: EpgEntryId::new(0),
            source_id: SourceId::new(1),
            channel: ChannelIdentity::from_raw(channel),
            title: title.to_owned(),
            description: None,
            start_unix: start,
            end_unix: end,
        }
    }

    #[test]
    fn replacement_is_windowed_and_now_next_is_stable() {
        let db = Db::open_in_memory().unwrap();
        let mut conn = db.writer();
        conn.execute(
            "INSERT INTO sources(id, kind, name) VALUES (1, 'm3u-url', 'Source')",
            [],
        )
        .unwrap();
        let entries = vec![
            entry(0, 50, "old"),
            entry(90, 110, "now"),
            entry(110, 130, "next"),
        ];
        assert_eq!(
            replace_window(&mut conn, SourceId::new(1), 80, 140, &entries).unwrap(),
            2
        );
        let (current, next) =
            now_next(&conn, SourceId::new(1), ChannelIdentity::from_raw(7), 100).unwrap();
        assert_eq!(current.unwrap().title, "now");
        assert_eq!(next.unwrap().title, "next");
    }

    #[test]
    fn batched_now_next_is_one_ordered_query_with_missing_and_repeated_channels() {
        let db = Db::open_in_memory().unwrap();
        let mut conn = db.writer();
        conn.execute(
            "INSERT INTO sources(id, kind, name) VALUES (1, 'm3u-url', 'Source')",
            [],
        )
        .unwrap();
        let entries = vec![
            entry_for(7, 90, 110, "Seven now"),
            entry_for(7, 110, 130, "Seven next"),
            entry_for(8, 105, 125, "Eight next"),
        ];
        replace_window(&mut conn, SourceId::new(1), 80, 140, &entries).unwrap();

        let requested = [
            ChannelIdentity::from_raw(8),
            ChannelIdentity::from_raw(404),
            ChannelIdentity::from_raw(7),
            ChannelIdentity::from_raw(7),
        ];
        let results = now_next_batch(&conn, SourceId::new(1), &requested, 100).unwrap();

        assert_eq!(results.len(), requested.len());
        assert_eq!(results[0].channel, requested[0]);
        assert!(results[0].current.is_none());
        assert_eq!(results[0].next.as_ref().unwrap().title, "Eight next");
        assert_eq!(
            results[1],
            EpgNowNext {
                channel: requested[1],
                current: None,
                next: None,
            }
        );
        for result in &results[2..] {
            assert_eq!(result.channel, requested[2]);
            assert_eq!(result.current.as_ref().unwrap().title, "Seven now");
            assert_eq!(result.next.as_ref().unwrap().title, "Seven next");
        }
    }

    #[test]
    fn dropping_or_orphaning_staging_never_replaces_the_live_guide() {
        let db = Db::open_in_memory().unwrap();
        {
            let conn = db.writer();
            conn.execute(
                "INSERT INTO sources(id, kind, name) VALUES (1, 'm3u-url', 'Source')",
                [],
            )
            .unwrap();
        }
        {
            let mut conn = db.writer();
            replace_window(&mut conn, SourceId::new(1), 0, 200, &[entry(10, 20, "old")]).unwrap();
        }

        let mut abandoned = db.begin_epg_staging(SourceId::new(1)).unwrap();
        abandoned.stage(&[entry(30, 40, "abandoned")]).unwrap();
        drop(abandoned);
        let conn = db.reader().unwrap();
        assert_eq!(
            list_window(
                &conn,
                SourceId::new(1),
                ChannelIdentity::from_raw(7),
                0,
                200,
                0,
                10,
            )
            .unwrap()[0]
                .title,
            "old"
        );
        drop(conn);

        let mut orphaned = db.begin_epg_staging(SourceId::new(1)).unwrap();
        orphaned.stage(&[entry(50, 60, "new")]).unwrap();
        db.writer()
            .execute("DELETE FROM sources WHERE id = 1", [])
            .unwrap();
        assert_eq!(
            orphaned.commit(&db, &|| false).unwrap(),
            EpgCommit::SourceRemoved
        );
    }

    #[test]
    fn a_commit_cancelled_at_the_writer_boundary_never_replaces_the_live_guide() {
        let db = Db::open_in_memory().unwrap();
        {
            let conn = db.writer();
            conn.execute(
                "INSERT INTO sources(id, kind, name) VALUES (1, 'm3u-url', 'Source')",
                [],
            )
            .unwrap();
        }
        {
            let mut conn = db.writer();
            replace_window(
                &mut conn,
                SourceId::new(1),
                0,
                200,
                &[entry(10, 20, "newer")],
            )
            .unwrap();
        }

        // A refresh superseded after it staged observes the cancellation under the writer lock, so
        // the swap is abandoned and the newer guide survives — this is the commit-boundary window.
        let mut superseded = db.begin_epg_staging(SourceId::new(1)).unwrap();
        superseded.stage(&[entry(30, 40, "stale")]).unwrap();
        assert_eq!(
            superseded.commit(&db, &|| true).unwrap(),
            EpgCommit::Superseded
        );
        {
            let conn = db.reader().unwrap();
            let live = list_window(
                &conn,
                SourceId::new(1),
                ChannelIdentity::from_raw(7),
                0,
                200,
                0,
                10,
            )
            .unwrap();
            assert_eq!(live.len(), 1);
            assert_eq!(live[0].title, "newer");
        }

        // An uncancelled commit still swaps the staged schedule in.
        let mut live_refresh = db.begin_epg_staging(SourceId::new(1)).unwrap();
        live_refresh.stage(&[entry(30, 40, "fresh")]).unwrap();
        assert_eq!(
            live_refresh.commit(&db, &|| false).unwrap(),
            EpgCommit::Committed { inserted: 1 }
        );
        {
            let conn = db.reader().unwrap();
            let live = list_window(
                &conn,
                SourceId::new(1),
                ChannelIdentity::from_raw(7),
                0,
                200,
                0,
                10,
            )
            .unwrap();
            assert_eq!(live.len(), 1);
            assert_eq!(live[0].title, "fresh");
        }
    }

    #[test]
    fn a_commit_cancelled_mid_swap_never_replaces_the_live_guide() {
        let db = Db::open_in_memory().unwrap();
        {
            let conn = db.writer();
            conn.execute(
                "INSERT INTO sources(id, kind, name) VALUES (1, 'm3u-url', 'Source')",
                [],
            )
            .unwrap();
        }
        {
            let mut conn = db.writer();
            replace_window(
                &mut conn,
                SourceId::new(1),
                0,
                200,
                &[entry(10, 20, "newer")],
            )
            .unwrap();
        }

        // The refresh is not yet superseded when the swap begins (first poll passes), but a newer
        // refresh cancels it while the DELETE/INSERT is being built — modelled by a predicate that
        // reports cancelled only from its second poll, which is the pre-commit re-check. The swap
        // must roll back to Superseded and leave the newer guide live.
        let polls = std::cell::Cell::new(0u32);
        let is_cancelled = || {
            polls.set(polls.get() + 1);
            polls.get() > 1
        };

        let mut superseded = db.begin_epg_staging(SourceId::new(1)).unwrap();
        superseded.stage(&[entry(30, 40, "stale")]).unwrap();
        assert_eq!(
            superseded.commit(&db, &is_cancelled).unwrap(),
            EpgCommit::Superseded
        );
        assert_eq!(polls.get(), 2, "the pre-commit re-check must have run");

        let conn = db.reader().unwrap();
        let live = list_window(
            &conn,
            SourceId::new(1),
            ChannelIdentity::from_raw(7),
            0,
            200,
            0,
            10,
        )
        .unwrap();
        assert_eq!(live.len(), 1);
        assert_eq!(live[0].title, "newer");
    }
}
