// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! `SearchService`: query in, ranked page out (TECH_SPEC §4.6, PRD §9 sub-50 ms budget).
//! Paged by contract; the ranking and FTS5 query compilation live in `core-search`.

use std::sync::Arc;

use core_db::Db;
use core_model::ids::SourceId;
use core_search::{SearchRequest, search};

use crate::error::ApiError;
use crate::records::{Channel, MediaKind, SearchPage};
use crate::runtime::CoreRuntime;

/// Runs channel searches against the FTS5 index.
#[derive(uniffi::Object)]
pub struct SearchService {
    rt: Arc<CoreRuntime>,
    db: Arc<Db>,
}

impl SearchService {
    /// Builds the service over shared runtime and database handles.
    pub(crate) fn new(rt: Arc<CoreRuntime>, db: Arc<Db>) -> Arc<Self> {
        Arc::new(Self { rt, db })
    }
}

#[uniffi::export]
impl SearchService {
    /// Searches channels, optionally filtered by source and/or media kind (paged by contract).
    ///
    /// # Errors
    /// Returns [`ApiError::StorageCorrupt`] on a query failure.
    pub async fn search(
        &self,
        query: String,
        source_id: Option<i64>,
        kind: Option<MediaKind>,
        offset: u32,
        limit: u32,
    ) -> Result<SearchPage, ApiError> {
        let db = Arc::clone(&self.db);
        self.rt
            .run_blocking(move || {
                let conn = db.reader()?;
                let request = SearchRequest {
                    text: &query,
                    source: source_id.map(SourceId::new),
                    kind: kind.map(Into::into),
                    offset,
                    limit,
                };
                let page = search(&conn, &request)?;
                Ok(SearchPage {
                    channels: page.channels.into_iter().map(Channel::from).collect(),
                    offset,
                    fuzzy: page.fuzzy,
                })
            })
            .await
    }
}
