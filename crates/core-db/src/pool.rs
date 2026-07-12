// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Connection pool: WAL, single writer / multiple readers (TECH_SPEC §4.4).
//!
//! SQLite in WAL mode allows one writer concurrent with many readers, so the pool models
//! exactly that: a single writer connection behind a `Mutex`, and a small bounded pool of
//! read-only connections. Every entry point here is **blocking** — the runtime's blocking
//! adapter (in `core-api`) is the only sanctioned caller, which is how the "no blocking on
//! async worker threads" rule is enforced structurally (rust-dev-pro.md async discipline).

use std::ops::Deref;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};

use rusqlite::{Connection, OpenFlags};

use crate::error::{DbError, DbResult};
use crate::migrations;

/// Default upper bound on cached reader connections. Readers beyond this open transiently
/// and are dropped on return rather than pooled.
const MAX_READERS: usize = 4;

/// A pooled SQLite database: one writer, a bounded set of readers, migrated to head.
pub struct Db {
    writer: Mutex<Connection>,
    readers: Mutex<Vec<Connection>>,
    open_reader: Box<dyn Fn() -> DbResult<Connection> + Send + Sync>,
    max_readers: usize,
}

impl Db {
    /// Opens (creating if absent) a file-backed database and migrates it to head.
    ///
    /// # Errors
    /// Returns [`DbError`] if the file cannot be opened, pragmas cannot be set, or
    /// migrations fail.
    pub fn open(path: &Path) -> DbResult<Self> {
        let owned = path.to_path_buf();
        let open_reader = Box::new(move || {
            let conn = Connection::open_with_flags(
                &owned,
                OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )
            .map_err(DbError::Connection)?;
            configure_reader(&conn)?;
            Ok(conn)
        });
        let mut writer = Connection::open(path).map_err(DbError::Connection)?;
        configure_writer(&writer)?;
        migrations::apply(&mut writer)?;
        Ok(Self::from_parts(writer, open_reader))
    }

    /// Opens a private in-memory database (shared-cache URI so readers see the writer's
    /// data) and migrates it to head. Intended for tests and ephemeral tooling.
    ///
    /// # Errors
    /// Returns [`DbError`] if the database cannot be opened or migrations fail.
    pub fn open_in_memory() -> DbResult<Self> {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let uri = format!("file:spidola-mem-{n}?mode=memory&cache=shared");
        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_URI
            | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let reader_uri = uri.clone();
        let open_reader = Box::new(move || {
            let conn = Connection::open_with_flags(
                &reader_uri,
                OpenFlags::SQLITE_OPEN_READ_ONLY
                    | OpenFlags::SQLITE_OPEN_URI
                    | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )
            .map_err(DbError::Connection)?;
            configure_reader(&conn)?;
            Ok(conn)
        });
        let mut writer = Connection::open_with_flags(&uri, flags).map_err(DbError::Connection)?;
        configure_writer(&writer)?;
        migrations::apply(&mut writer)?;
        Ok(Self::from_parts(writer, open_reader))
    }

    fn from_parts(
        writer: Connection,
        open_reader: Box<dyn Fn() -> DbResult<Connection> + Send + Sync>,
    ) -> Self {
        Self {
            writer: Mutex::new(writer),
            readers: Mutex::new(Vec::new()),
            open_reader,
            max_readers: MAX_READERS,
        }
    }

    /// Acquires the single writer connection, blocking until it is free.
    ///
    /// Poisoning (a panic while the lock was held) is recovered from rather than
    /// propagated, since a SQLite connection has no invariant a panic could corrupt beyond
    /// what the transaction already rolled back.
    pub fn writer(&self) -> MutexGuard<'_, Connection> {
        self.writer.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Checks out a read-only connection, reusing a pooled one when available.
    ///
    /// # Errors
    /// Returns [`DbError`] if a fresh reader must be opened and that open fails.
    pub fn reader(&self) -> DbResult<ReaderGuard<'_>> {
        let pooled = {
            let mut readers = self.readers.lock().unwrap_or_else(PoisonError::into_inner);
            readers.pop()
        };
        let conn = match pooled {
            Some(conn) => conn,
            None => (self.open_reader)()?,
        };
        Ok(ReaderGuard {
            conn: Some(conn),
            pool: self,
        })
    }

    fn recycle(&self, conn: Connection) {
        let mut readers = self.readers.lock().unwrap_or_else(PoisonError::into_inner);
        if readers.len() < self.max_readers {
            readers.push(conn);
        }
        // else: drop the transient reader, closing it.
    }
}

/// RAII handle to a checked-out read-only connection; returns it to the pool on drop.
pub struct ReaderGuard<'a> {
    conn: Option<Connection>,
    pool: &'a Db,
}

impl Deref for ReaderGuard<'_> {
    type Target = Connection;

    #[allow(clippy::expect_used)] // Invariant: `conn` is `Some` for the guard's whole life;
    // only `Drop` takes it. This mirrors the standard pool-guard pattern.
    fn deref(&self) -> &Self::Target {
        self.conn
            .as_ref()
            .expect("reader connection present until drop")
    }
}

impl Drop for ReaderGuard<'_> {
    fn drop(&mut self) {
        if let Some(conn) = self.conn.take() {
            self.pool.recycle(conn);
        }
    }
}

fn configure_writer(conn: &Connection) -> DbResult<()> {
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA foreign_keys = ON;
         PRAGMA busy_timeout = 5000;
         PRAGMA synchronous = NORMAL;",
    )
    .map_err(DbError::Connection)
}

fn configure_reader(conn: &Connection) -> DbResult<()> {
    conn.execute_batch(
        "PRAGMA foreign_keys = ON;
         PRAGMA busy_timeout = 5000;
         PRAGMA query_only = ON;",
    )
    .map_err(DbError::Connection)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn writer_and_readers_share_the_same_data() {
        let db = Db::open_in_memory().unwrap();
        {
            let w = db.writer();
            w.execute(
                "INSERT INTO sources(id, kind, name) VALUES (1, 'm3u-url', 'S')",
                [],
            )
            .unwrap();
        }
        let reader = db.reader().unwrap();
        let n: i64 = reader
            .query_row("SELECT count(*) FROM sources", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn readers_are_query_only() {
        let db = Db::open_in_memory().unwrap();
        let reader = db.reader().unwrap();
        let err = reader.execute(
            "INSERT INTO sources(id, kind, name) VALUES (2, 'm3u-url', 'X')",
            [],
        );
        assert!(err.is_err(), "a reader must reject writes");
    }

    #[test]
    fn readers_are_recycled_up_to_the_cap() {
        let db = Db::open_in_memory().unwrap();
        // Exercise checkout/return several times; must never error.
        for _ in 0..10 {
            let reader = db.reader().unwrap();
            let _: i64 = reader
                .query_row("SELECT count(*) FROM channels", [], |r| r.get(0))
                .unwrap();
        }
        let cached = db.readers.lock().unwrap().len();
        assert!(cached <= MAX_READERS);
    }
}
