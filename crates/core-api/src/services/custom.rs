// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Core-owned custom channels and portable sharing (PRD §6.7).

use std::collections::HashMap;
use std::sync::Arc;

use core_db::{Db, repo};
use core_model::{CustomChannel, CustomChannelId, CustomGroupId, StreamLocator};
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::error::{ApiError, InputField, InputIssue};
use crate::records::{
    CustomChannelDraft, CustomChannelExport, CustomChannelSummary, CustomGroup, CustomImportMode,
    ResolvedHeader, ResolvedStream,
};
use crate::runtime::CoreRuntime;
use crate::storage_crypto::CatalogCipher;

const PORTABLE_VERSION: u32 = 1;
const MAX_PORTABLE_BYTES: usize = 2 * 1024 * 1024;
const MAX_PORTABLE_GROUPS: usize = 1_024;
const MAX_PORTABLE_CHANNELS: usize = 10_000;
const MAX_CHANNEL_HEADERS: usize = 64;
const MAX_FIELD_BYTES: usize = 64 * 1024;

#[derive(uniffi::Object)]
pub struct CustomChannelService {
    rt: Arc<CoreRuntime>,
    db: Arc<Db>,
    cipher: Arc<CatalogCipher>,
}

impl CustomChannelService {
    pub(crate) fn new(rt: Arc<CoreRuntime>, db: Arc<Db>, cipher: Arc<CatalogCipher>) -> Arc<Self> {
        Arc::new(Self { rt, db, cipher })
    }
}

#[uniffi::export]
impl CustomChannelService {
    /// Creates a named custom group at the end of the lineup.
    ///
    /// # Errors
    /// Returns an input error for a blank name or a storage error if insertion fails.
    #[instrument(skip(self), err)]
    pub async fn create_group(&self, name: String) -> Result<i64, ApiError> {
        let name = normalized_name(&name)?;
        let db = Arc::clone(&self.db);
        self.rt
            .run_blocking(move || {
                let conn = db.writer();
                Ok(repo::custom::create_group(&conn, &name)?.value())
            })
            .await
    }

    /// Lists a bounded page of groups in user-defined order.
    ///
    /// # Errors
    /// Returns a storage error if the page cannot be read.
    pub async fn groups(&self, offset: u32, limit: u32) -> Result<Vec<CustomGroup>, ApiError> {
        let db = Arc::clone(&self.db);
        self.rt
            .run_blocking(move || {
                let conn = db.reader()?;
                Ok(repo::custom::list_groups(&conn, offset, limit)?
                    .into_iter()
                    .map(CustomGroup::from)
                    .collect())
            })
            .await
    }

    /// Renames an existing group.
    ///
    /// # Errors
    /// Returns `NotFound`, an input error, or a storage error.
    pub async fn rename_group(&self, id: i64, name: String) -> Result<(), ApiError> {
        let name = normalized_name(&name)?;
        let db = Arc::clone(&self.db);
        self.rt
            .run_blocking(move || {
                let conn = db.writer();
                if repo::custom::rename_group(&conn, CustomGroupId::new(id), &name)? {
                    Ok(())
                } else {
                    Err(ApiError::NotFound)
                }
            })
            .await
    }

    /// Deletes a group while keeping its channels in the ungrouped lineup.
    ///
    /// # Errors
    /// Returns `NotFound` or a storage error.
    pub async fn delete_group(&self, id: i64) -> Result<(), ApiError> {
        let db = Arc::clone(&self.db);
        self.rt
            .run_blocking(move || {
                let conn = db.writer();
                if repo::custom::delete_group(&conn, CustomGroupId::new(id))? {
                    Ok(())
                } else {
                    Err(ApiError::NotFound)
                }
            })
            .await
    }

    /// Moves a group immediately before another.
    ///
    /// # Errors
    /// Returns `NotFound` or a storage error.
    pub async fn move_group_before(&self, id: i64, anchor_id: i64) -> Result<(), ApiError> {
        self.move_group_relative(id, anchor_id, false).await
    }

    /// Moves a group immediately after another.
    ///
    /// # Errors
    /// Returns `NotFound` or a storage error.
    pub async fn move_group_after(&self, id: i64, anchor_id: i64) -> Result<(), ApiError> {
        self.move_group_relative(id, anchor_id, true).await
    }

    #[instrument(skip(self, draft), err)]
    /// Creates a sealed custom channel.
    ///
    /// # Errors
    /// Returns an input, not-found, storage, or secure-store error.
    pub async fn create(&self, draft: Arc<CustomChannelDraft>) -> Result<i64, ApiError> {
        let channel = sealed_channel(&self.cipher, CustomChannelId::new(0), &draft)?;
        let db = Arc::clone(&self.db);
        self.rt
            .run_blocking(move || {
                let conn = db.writer();
                if let Some(group) = channel.group_id
                    && !repo::custom::group_exists(&conn, group)?
                {
                    return Err(ApiError::NotFound);
                }
                Ok(repo::custom::create_channel(&conn, &channel)?.value())
            })
            .await
    }

    #[instrument(skip(self, draft), err)]
    /// Replaces an existing channel's editable fields.
    ///
    /// # Errors
    /// Returns an input, not-found, storage, or secure-store error.
    pub async fn update(&self, id: i64, draft: Arc<CustomChannelDraft>) -> Result<(), ApiError> {
        let channel = sealed_channel(&self.cipher, CustomChannelId::new(id), &draft)?;
        let db = Arc::clone(&self.db);
        self.rt
            .run_blocking(move || {
                let conn = db.writer();
                if let Some(group) = channel.group_id
                    && !repo::custom::group_exists(&conn, group)?
                {
                    return Err(ApiError::NotFound);
                }
                if repo::custom::update_channel(&conn, &channel)? {
                    Ok(())
                } else {
                    Err(ApiError::NotFound)
                }
            })
            .await
    }

    /// Moves a channel immediately before another, including across groups.
    ///
    /// # Errors
    /// Returns `NotFound` or a storage error.
    pub async fn move_before(&self, id: i64, anchor_id: i64) -> Result<(), ApiError> {
        self.move_relative(id, anchor_id, false).await
    }

    /// Moves a channel immediately after another, including across groups.
    ///
    /// # Errors
    /// Returns `NotFound` or a storage error.
    pub async fn move_after(&self, id: i64, anchor_id: i64) -> Result<(), ApiError> {
        self.move_relative(id, anchor_id, true).await
    }

    /// Deletes a custom channel idempotently.
    ///
    /// # Errors
    /// Returns a storage error if the delete fails.
    pub async fn delete(&self, id: i64) -> Result<(), ApiError> {
        let db = Arc::clone(&self.db);
        self.rt
            .run_blocking(move || {
                let conn = db.writer();
                repo::custom::delete_channel(&conn, CustomChannelId::new(id))?;
                Ok(())
            })
            .await
    }

    /// Lists one group's custom channels in user-defined order.
    ///
    /// # Errors
    /// Returns a storage error if the page cannot be decoded.
    pub async fn list(
        &self,
        group_id: Option<i64>,
        offset: u32,
        limit: u32,
    ) -> Result<Vec<CustomChannelSummary>, ApiError> {
        let db = Arc::clone(&self.db);
        self.rt
            .run_blocking(move || {
                let conn = db.reader()?;
                Ok(repo::custom::list_channels(
                    &conn,
                    group_id.map(CustomGroupId::new),
                    offset,
                    limit,
                )?
                .into_iter()
                .map(CustomChannelSummary::from)
                .collect())
            })
            .await
    }

    /// Opens a custom channel only for immediate engine construction.
    ///
    /// # Errors
    /// Returns `NotFound`, a storage error, or an integrity error for a damaged envelope.
    pub async fn resolve(&self, id: i64) -> Result<Arc<ResolvedStream>, ApiError> {
        let db = Arc::clone(&self.db);
        let cipher = Arc::clone(&self.cipher);
        self.rt
            .run_blocking(move || {
                let conn = db.reader()?;
                let channel = repo::custom::get_channel(&conn, CustomChannelId::new(id))?
                    .ok_or(ApiError::NotFound)?;
                let locator = cipher.open_sealed_locator(&channel.locator)?;
                let user_agent = channel
                    .user_agent
                    .map(|value| cipher.open_sealed_value(&value))
                    .transpose()?;
                let headers = channel
                    .headers
                    .into_iter()
                    .map(|(name, value)| {
                        cipher
                            .open_sealed_value(&value)
                            .map(|value| ResolvedHeader::new(name, value))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(ResolvedStream::new(
                    locator.as_str().to_owned(),
                    user_agent,
                    headers,
                ))
            })
            .await
    }

    /// Produces a versioned portable document. The returned object redacts diagnostics.
    ///
    /// # Errors
    /// Returns a storage or integrity error if the catalog cannot be exported safely.
    pub async fn export_portable(&self) -> Result<Arc<CustomChannelExport>, ApiError> {
        let db = Arc::clone(&self.db);
        let cipher = Arc::clone(&self.cipher);
        self.rt
            .run_blocking(move || {
                let conn = db.reader()?;
                let groups = repo::custom::list_groups(&conn, 0, u32::MAX)?;
                let group_names = groups
                    .iter()
                    .map(|group| (group.id, group.name.clone()))
                    .collect::<HashMap<_, _>>();
                let channels = repo::custom::list_all_channels(&conn)?
                    .into_iter()
                    .map(|channel| portable_channel(&cipher, channel, &group_names))
                    .collect::<Result<Vec<_>, _>>()?;
                let contents = serde_json::to_string_pretty(&PortableCatalog {
                    version: PORTABLE_VERSION,
                    groups: groups.into_iter().map(|group| group.name).collect(),
                    channels,
                })
                .map_err(|_| ApiError::Internal)?;
                Ok(CustomChannelExport::new(contents))
            })
            .await
    }

    /// Imports a versioned portable document with explicit conflict behavior.
    ///
    /// # Errors
    /// Returns an input, storage, or integrity error. Replace mode is atomic.
    #[instrument(skip(self, contents), err)]
    pub async fn import_portable(
        &self,
        contents: String,
        mode: CustomImportMode,
    ) -> Result<u64, ApiError> {
        if contents.len() > MAX_PORTABLE_BYTES {
            return Err(ApiError::InvalidInput {
                field: InputField::File,
                issue: InputIssue::Unsupported,
            });
        }
        let catalog: PortableCatalog =
            serde_json::from_str(&contents).map_err(|_| ApiError::InvalidInput {
                field: InputField::File,
                issue: InputIssue::Invalid,
            })?;
        if catalog.version != PORTABLE_VERSION {
            return Err(ApiError::InvalidInput {
                field: InputField::File,
                issue: InputIssue::Unsupported,
            });
        }
        if catalog.groups.len() > MAX_PORTABLE_GROUPS
            || catalog.channels.len() > MAX_PORTABLE_CHANNELS
        {
            return Err(ApiError::InvalidInput {
                field: InputField::File,
                issue: InputIssue::Unsupported,
            });
        }
        let db = Arc::clone(&self.db);
        let cipher = Arc::clone(&self.cipher);
        self.rt
            .run_blocking(move || import_catalog(&db, &cipher, catalog, mode))
            .await
    }
}

impl CustomChannelService {
    async fn move_group_relative(
        &self,
        id: i64,
        anchor_id: i64,
        after: bool,
    ) -> Result<(), ApiError> {
        let db = Arc::clone(&self.db);
        self.rt
            .run_blocking(move || {
                let mut conn = db.writer();
                let moved = if after {
                    repo::custom::move_group_after(
                        &mut conn,
                        CustomGroupId::new(id),
                        CustomGroupId::new(anchor_id),
                    )?
                } else {
                    repo::custom::move_group_before(
                        &mut conn,
                        CustomGroupId::new(id),
                        CustomGroupId::new(anchor_id),
                    )?
                };
                if moved {
                    Ok(())
                } else {
                    Err(ApiError::NotFound)
                }
            })
            .await
    }

    async fn move_relative(&self, id: i64, anchor_id: i64, after: bool) -> Result<(), ApiError> {
        let db = Arc::clone(&self.db);
        self.rt
            .run_blocking(move || {
                let mut conn = db.writer();
                let moved = if after {
                    repo::custom::move_channel_after(
                        &mut conn,
                        CustomChannelId::new(id),
                        CustomChannelId::new(anchor_id),
                    )?
                } else {
                    repo::custom::move_channel_before(
                        &mut conn,
                        CustomChannelId::new(id),
                        CustomChannelId::new(anchor_id),
                    )?
                };
                if moved {
                    Ok(())
                } else {
                    Err(ApiError::NotFound)
                }
            })
            .await
    }
}

fn normalized_name(value: &str) -> Result<String, ApiError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(ApiError::InvalidInput {
            field: InputField::Name,
            issue: InputIssue::Empty,
        });
    }
    if value.len() > MAX_FIELD_BYTES {
        return Err(ApiError::InvalidInput {
            field: InputField::Name,
            issue: InputIssue::Unsupported,
        });
    }
    Ok(value.to_owned())
}

fn validate_overrides(
    user_agent: Option<&str>,
    headers: &[(String, String)],
) -> Result<(), ApiError> {
    if headers.len() > MAX_CHANNEL_HEADERS
        || user_agent.is_some_and(|value| value.len() > MAX_FIELD_BYTES)
        || headers
            .iter()
            .any(|(name, value)| name.len() > MAX_FIELD_BYTES || value.len() > MAX_FIELD_BYTES)
    {
        return Err(ApiError::InvalidInput {
            field: InputField::Header,
            issue: InputIssue::Unsupported,
        });
    }
    core_fetch::validate_headers(user_agent, headers).map_err(ApiError::from)
}

fn sealed_channel(
    cipher: &CatalogCipher,
    id: CustomChannelId,
    draft: &CustomChannelDraft,
) -> Result<CustomChannel, ApiError> {
    let name = normalized_name(&draft.name)?;
    let locator = StreamLocator::parse(&draft.locator).map_err(ApiError::from)?;
    let locator = cipher.seal_locator(&locator)?;
    let plain_headers = draft
        .headers
        .iter()
        .map(|header| {
            let (name, value) = header.pair();
            (name.to_owned(), value.to_owned())
        })
        .collect::<Vec<_>>();
    validate_overrides(draft.user_agent.as_deref(), &plain_headers)?;
    let user_agent = draft
        .user_agent
        .as_deref()
        .filter(|value| !value.is_empty())
        .map(|value| cipher.seal_value(value))
        .transpose()?;
    let headers = draft
        .headers
        .iter()
        .map(|header| {
            let (name, value) = header.pair();
            cipher
                .seal_value(value)
                .map(|value| (name.to_owned(), value))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(CustomChannel {
        id,
        group_id: draft.group_id.map(CustomGroupId::new),
        name,
        logo: draft.logo.clone().filter(|value| !value.trim().is_empty()),
        locator,
        user_agent,
        headers,
        position: 0,
    })
}

#[derive(Serialize, Deserialize)]
struct PortableCatalog {
    version: u32,
    #[serde(default)]
    groups: Vec<String>,
    channels: Vec<PortableChannel>,
}

#[derive(Serialize, Deserialize)]
struct PortableChannel {
    group: Option<String>,
    name: String,
    logo: Option<String>,
    locator: String,
    user_agent: Option<String>,
    headers: Vec<(String, String)>,
}

fn portable_channel(
    cipher: &CatalogCipher,
    channel: CustomChannel,
    groups: &HashMap<CustomGroupId, String>,
) -> Result<PortableChannel, ApiError> {
    Ok(PortableChannel {
        group: channel.group_id.and_then(|id| groups.get(&id).cloned()),
        name: channel.name,
        logo: channel.logo,
        locator: cipher
            .open_sealed_locator(&channel.locator)?
            .as_str()
            .to_owned(),
        user_agent: channel
            .user_agent
            .map(|value| cipher.open_sealed_value(&value))
            .transpose()?,
        headers: channel
            .headers
            .into_iter()
            .map(|(name, value)| cipher.open_sealed_value(&value).map(|value| (name, value)))
            .collect::<Result<Vec<_>, _>>()?,
    })
}

fn import_catalog(
    db: &Db,
    cipher: &CatalogCipher,
    catalog: PortableCatalog,
    mode: CustomImportMode,
) -> Result<u64, ApiError> {
    let mut conn = db.writer();
    let transaction = conn
        .transaction()
        .map_err(core_db::DbError::from)
        .map_err(ApiError::from)?;
    if mode == CustomImportMode::Replace {
        repo::custom::clear(&transaction)?;
    }
    let mut group_ids = HashMap::new();
    for name in catalog.groups {
        let name = normalized_name(&name)?;
        let id = match repo::custom::group_id_by_name(&transaction, &name)? {
            Some(id) => id,
            None => repo::custom::create_group(&transaction, &name)?,
        };
        group_ids.insert(name, id);
    }
    let mut imported = 0_u64;
    for portable in catalog.channels {
        let normalized_group = portable.group.as_deref().map(normalized_name).transpose()?;
        let group_id = if let Some(name) = normalized_group.as_deref() {
            if let Some(id) = group_ids.get(name).copied() {
                Some(id)
            } else {
                let id = match repo::custom::group_id_by_name(&transaction, name)? {
                    Some(id) => id,
                    None => repo::custom::create_group(&transaction, name)?,
                };
                group_ids.insert(name.to_owned(), id);
                Some(id)
            }
        } else {
            None
        };
        validate_overrides(portable.user_agent.as_deref(), &portable.headers)?;
        let locator = StreamLocator::parse(&portable.locator).map_err(ApiError::from)?;
        let channel = CustomChannel {
            id: CustomChannelId::new(0),
            group_id,
            name: normalized_name(&portable.name)?,
            logo: portable.logo,
            locator: cipher.seal_locator(&locator)?,
            user_agent: portable
                .user_agent
                .map(|value| cipher.seal_value(&value))
                .transpose()?,
            headers: portable
                .headers
                .into_iter()
                .map(|(name, value)| cipher.seal_value(&value).map(|value| (name, value)))
                .collect::<Result<Vec<_>, _>>()?,
            position: 0,
        };
        repo::custom::create_channel(&transaction, &channel)?;
        imported = imported.saturating_add(1);
    }
    transaction
        .commit()
        .map_err(core_db::DbError::from)
        .map_err(ApiError::from)?;
    Ok(imported)
}
