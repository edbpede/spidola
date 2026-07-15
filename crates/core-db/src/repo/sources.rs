// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Sources repository (TECH_SPEC §4.4). Persists everything about a source **except its
//! secret**: the Xtream variant stores `username` + an opaque `secret_ref`, never the
//! password (§12).

use rusqlite::{Connection, Row, params};

use core_model::ids::{SecretRef, SourceId};
use core_model::locator::StreamLocator;
use core_model::source::{Source, SourceCommon, SourceKind};

use crate::error::{DbError, DbResult};

const fn kind_to_str(kind: SourceKind) -> &'static str {
    match kind {
        SourceKind::M3uUrl => "m3u-url",
        SourceKind::M3uFile => "m3u-file",
        SourceKind::Xtream => "xtream",
    }
}

fn parse_locator(raw: &str) -> DbResult<StreamLocator> {
    StreamLocator::parse(raw).map_err(|e| DbError::Integrity(format!("stored URL is invalid: {e}")))
}

/// Inserts a source, returning its assigned identity. The `id` on `source` is ignored (the
/// database mints the rowid).
///
/// # Errors
/// Returns [`DbError`] on a write failure.
pub fn insert(conn: &Connection, source: &Source) -> DbResult<SourceId> {
    let common = source.common();
    let (url, username, secret_ref, has_user_agent, accept_invalid_tls) = match source {
        Source::M3uUrl {
            url_secret,
            has_user_agent,
            accept_invalid_tls,
            ..
        } => (
            None,
            None,
            Some(url_secret.as_str().to_owned()),
            *has_user_agent,
            *accept_invalid_tls,
        ),
        Source::M3uFile { .. } => (None, None, None, false, false),
        Source::Xtream {
            server,
            username,
            secret,
            ..
        } => (
            Some(server.as_str().to_owned()),
            Some(username.clone()),
            Some(secret.as_str().to_owned()),
            false,
            false,
        ),
    };
    conn.execute(
        "INSERT INTO sources(kind, name, enabled, auto_refresh_secs, url, username, \
         secret_ref, user_agent, has_user_agent, accept_invalid_tls) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            kind_to_str(source.kind()),
            common.name,
            common.enabled,
            common.auto_refresh_secs,
            url,
            username,
            secret_ref,
            Option::<String>::None,
            has_user_agent,
            accept_invalid_tls,
        ],
    )?;
    Ok(SourceId::new(conn.last_insert_rowid()))
}

fn map_source(row: &Row<'_>) -> DbResult<Source> {
    let id = SourceId::new(row.get("id")?);
    let common = SourceCommon {
        name: row.get("name")?,
        enabled: row.get("enabled")?,
        auto_refresh_secs: row
            .get::<_, Option<i64>>("auto_refresh_secs")?
            .and_then(|v| u32::try_from(v).ok()),
    };
    let kind: String = row.get("kind")?;
    match kind.as_str() {
        "m3u-url" => Ok(Source::M3uUrl {
            id,
            common,
            url_secret: SecretRef::new(row.get::<_, Option<String>>("secret_ref")?.ok_or_else(
                || DbError::Integrity("M3U URL source has no secure reference".to_owned()),
            )?),
            has_user_agent: row.get("has_user_agent")?,
            accept_invalid_tls: row.get("accept_invalid_tls")?,
        }),
        "m3u-file" => Ok(Source::M3uFile { id, common }),
        "xtream" => Ok(Source::Xtream {
            id,
            common,
            server: parse_locator(&row.get::<_, String>("url")?)?,
            username: row
                .get::<_, Option<String>>("username")?
                .unwrap_or_default(),
            secret: SecretRef::new(
                row.get::<_, Option<String>>("secret_ref")?
                    .unwrap_or_default(),
            ),
        }),
        other => Err(DbError::Integrity(format!("unknown source kind `{other}`"))),
    }
}

const SELECT_COLUMNS: &str = "id, kind, name, enabled, auto_refresh_secs, url, username, \
                              secret_ref, user_agent, has_user_agent, accept_invalid_tls";

/// Fetches one source by id.
///
/// # Errors
/// Returns [`DbError`] on a query or mapping failure.
pub fn get(conn: &Connection, id: SourceId) -> DbResult<Option<Source>> {
    let sql = format!("SELECT {SELECT_COLUMNS} FROM sources WHERE id = ?1");
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query(params![id.value()])?;
    match rows.next()? {
        Some(row) => Ok(Some(map_source(row)?)),
        None => Ok(None),
    }
}

/// Lists all sources, newest first.
///
/// # Errors
/// Returns [`DbError`] on a query or mapping failure.
pub fn list(conn: &Connection) -> DbResult<Vec<Source>> {
    let sql = format!("SELECT {SELECT_COLUMNS} FROM sources ORDER BY id DESC");
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query([])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(map_source(row)?);
    }
    Ok(out)
}

/// Renames a source.
///
/// # Errors
/// Returns [`DbError`] on a write failure.
pub fn rename(conn: &Connection, id: SourceId, name: &str) -> DbResult<()> {
    conn.execute(
        "UPDATE sources SET name = ?2 WHERE id = ?1",
        params![id.value(), name],
    )?;
    Ok(())
}

/// Enables or disables a source without deleting it (PRD §6.1).
///
/// # Errors
/// Returns [`DbError`] on a write failure.
pub fn set_enabled(conn: &Connection, id: SourceId, enabled: bool) -> DbResult<()> {
    conn.execute(
        "UPDATE sources SET enabled = ?2 WHERE id = ?1",
        params![id.value(), enabled],
    )?;
    Ok(())
}

/// Sets (or clears, with `None`) the automatic refresh interval.
///
/// # Errors
/// Returns [`DbError`] on a write failure.
pub fn set_auto_refresh(conn: &Connection, id: SourceId, secs: Option<u32>) -> DbResult<()> {
    conn.execute(
        "UPDATE sources SET auto_refresh_secs = ?2 WHERE id = ?1",
        params![id.value(), secs],
    )?;
    Ok(())
}

/// Whether a source with `id` still exists.
///
/// The writer-free refresh ([`crate::refresh`]) calls this under the writer, inside the swap
/// transaction, as its serialization point: if a concurrent delete removed the source while
/// the catalog was staged off-lock, the swap is abandoned cleanly instead of resurrecting a
/// vanished source or tripping the `channels.source_id` foreign key.
///
/// # Errors
/// Returns [`DbError`] on a query failure.
pub fn exists(conn: &Connection, id: SourceId) -> DbResult<bool> {
    let n: i64 = conn.query_row(
        "SELECT count(*) FROM sources WHERE id = ?1",
        params![id.value()],
        |r| r.get(0),
    )?;
    Ok(n > 0)
}

/// Deletes a source and (by cascade) its catalog, favorites, hidden flags, and history.
///
/// # Errors
/// Returns [`DbError`] on a write failure.
pub fn delete(conn: &Connection, id: SourceId) -> DbResult<()> {
    conn.execute("DELETE FROM sources WHERE id = ?1", params![id.value()])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::pool::Db;

    fn m3u(name: &str) -> Source {
        Source::M3uUrl {
            id: SourceId::new(0),
            common: SourceCommon {
                name: name.to_owned(),
                enabled: true,
                auto_refresh_secs: None,
            },
            url_secret: SecretRef::new("m3u-url/test/url"),
            has_user_agent: true,
            accept_invalid_tls: false,
        }
    }

    #[test]
    fn insert_get_list_roundtrip() {
        let db = Db::open_in_memory().unwrap();
        let conn = db.writer();
        let id = insert(&conn, &m3u("Home")).unwrap();
        let fetched = get(&conn, id).unwrap().unwrap();
        assert_eq!(fetched.id(), id);
        assert_eq!(fetched.common().name, "Home");
        assert_eq!(list(&conn).unwrap().len(), 1);
    }

    #[test]
    fn xtream_persists_reference_not_password() {
        let db = Db::open_in_memory().unwrap();
        let conn = db.writer();
        let src = Source::Xtream {
            id: SourceId::new(0),
            common: SourceCommon {
                name: "Panel".to_owned(),
                enabled: true,
                auto_refresh_secs: Some(7200),
            },
            server: StreamLocator::parse("http://panel.example:8080").unwrap(),
            username: "alice".to_owned(),
            secret: SecretRef::new("xtream/1/password"),
        };
        let id = insert(&conn, &src).unwrap();
        let back = get(&conn, id).unwrap().unwrap();
        match back {
            Source::Xtream {
                username, secret, ..
            } => {
                assert_eq!(username, "alice");
                assert_eq!(secret.as_str(), "xtream/1/password");
            }
            other => panic!("wrong variant: {other:?}"),
        }
        // The password itself is nowhere in the row.
        let stored: String = conn
            .query_row("SELECT coalesce(secret_ref,'') FROM sources", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(stored, "xtream/1/password");
    }

    #[test]
    fn mutations_apply() {
        let db = Db::open_in_memory().unwrap();
        let conn = db.writer();
        let id = insert(&conn, &m3u("A")).unwrap();
        rename(&conn, id, "B").unwrap();
        set_enabled(&conn, id, false).unwrap();
        set_auto_refresh(&conn, id, Some(3600)).unwrap();
        let s = get(&conn, id).unwrap().unwrap();
        assert_eq!(s.common().name, "B");
        assert!(!s.common().enabled);
        assert_eq!(s.common().auto_refresh_secs, Some(3600));
        delete(&conn, id).unwrap();
        assert!(get(&conn, id).unwrap().is_none());
    }
}
