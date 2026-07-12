// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! `core-db` — SQLite persistence; every entry point is a blocking function (TECH_SPEC §4.4).
//!
//! rusqlite (bundled SQLite, FTS5) behind our own blocking API: the connection pool runs
//! WAL with one writer and many readers, migrations are forward-only and numbered, channel
//! refresh is staging-and-swap (a failure leaves the prior catalog intact), and favorites /
//! hidden flags key on the stable channel identity so they survive a refresh. Secrets never
//! land here — the DB stores opaque `SecretRef` keys only (§12).
#![forbid(unsafe_code)]

pub mod error;
pub mod migrations;
pub mod pool;
pub mod refresh;
pub mod repo;
pub mod search_index;

pub use error::{DbError, DbResult};
pub use migrations::SCHEMA_VERSION;
pub use pool::{Db, ReaderGuard};
pub use refresh::{Refresh, RefreshOutcome};
pub use repo::channels::NewChannel;
pub use repo::history::NewHistory;
