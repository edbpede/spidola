// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Defensive deserialization of wild responses (numbers-as-strings, missing fields).
//!
//! Xtream is a de-facto protocol with no specification and a decade of forks, so the same
//! logical field arrives as `1`, `"1"`, `true`, `null`, `""`, or not at all depending on
//! which panel a user bought (TECH_SPEC §4.3). Rather than model each vendor's quirks, the
//! DTOs here declare *what a field means* and delegate *how it was spelled* to the
//! `de_flex_*` family, which accepts every encoding seen in the wild and normalizes it.
//!
//! Three rules shape this module:
//!
//! - **Every field is optional.** A missing or unreadable field yields `None` and the
//!   mapper decides whether the row survives, so the tolerance policy lives with the
//!   mapping (`crate::catalog`) rather than being scattered through `serde` attributes.
//! - **Unknown fields are ignored**, never denied. This is what lets one DTO read a dozen
//!   panel forks — and it is also load-bearing for §12: Xtream's `user_info` mirrors the
//!   account *password* back in the response, and because [`UserInfo`] simply never
//!   declares that field, the credential is dropped on the floor at the parse boundary and
//!   has nowhere in this crate to live.
//! - **Nothing here is public.** These types are Xtream's shape, and §4.3 requires that
//!   nothing Xtream-shaped leaks upward; `crate::catalog` and `crate::series` map them into
//!   `core-model` and the DTOs stop at the crate boundary.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer};
use serde_json::Value;

use crate::error::{XtreamError, XtreamResult};

/// Reads a listing envelope: the endpoint promised an array, so anything else is an
/// [`XtreamError::Malformed`].
///
/// The rows come back as raw [`Value`]s rather than DTOs on purpose. Deserializing straight
/// into `Vec<StreamDto>` would make serde abandon the *whole* array on the first unreadable
/// row, which is exactly the failure mode §4.3 forbids; reading rows one at a time lets
/// `crate::catalog` skip-and-count the bad one and keep the other 49,999.
///
/// Some panels answer an error as `{"user_info": {...}}` even for a listing action, so an
/// object here is reported as the authentication failure it usually is.
pub(crate) fn parse_rows(body: &[u8]) -> XtreamResult<Vec<Value>> {
    let value: Value = serde_json::from_slice(body).map_err(|e| XtreamError::Malformed {
        detail: format!("the response is not JSON ({e})"),
    })?;
    match value {
        Value::Array(rows) => Ok(rows),
        other => Err(XtreamError::Malformed {
            detail: format!("expected a list of entries, found {}", json_type_of(&other)),
        }),
    }
}

/// Reads a single-object envelope (the handshake, `get_series_info`) into `T`.
pub(crate) fn parse_object<T: DeserializeOwned>(body: &[u8]) -> XtreamResult<T> {
    serde_json::from_slice(body).map_err(|e| XtreamError::Malformed {
        detail: format!("the response is not the expected object ({e})"),
    })
}

/// Any JSON scalar, in the order `serde` should try to read it.
///
/// The whole `de_flex_*` family funnels through this: read the scalar however it was
/// spelled, then interpret it. An object or array where a scalar belongs fails to
/// deserialize, which the mapper records as a skipped row.
#[derive(Deserialize)]
#[serde(untagged)]
enum Scalar {
    Bool(bool),
    Unsigned(u64),
    Signed(i64),
    Float(f64),
    Text(String),
}

/// `2^64` — the smallest value too large for a `u64`.
///
/// Written as a literal rather than `u64::MAX as f64` so the bound itself is exact: the cast
/// would round `u64::MAX` *up* to this same value, making the comparison admit a float one
/// step beyond the range it claims to guard.
const U64_LIMIT: f64 = 18_446_744_073_709_551_616.0;

/// `2^63` — the smallest value too large for an `i64`, and the exact magnitude of
/// `i64::MIN`. Exact in `f64` for the same reason as [`U64_LIMIT`]: it is a power of two.
const I64_LIMIT: f64 = 9_223_372_036_854_775_808.0;

impl Scalar {
    /// Reads the scalar as an unsigned integer, or `None` if it does not denote one.
    ///
    /// A negative or fractional number is not an id or a count, so it reads as absent
    /// rather than being truncated into a plausible-looking lie.
    ///
    /// The float arm exists for `123.0`-style spellings: `serde_json` already hands whole
    /// in-range numbers to [`Self::Unsigned`], so anything reaching here is fractional,
    /// negative, or beyond the range — and the guard rejects all three.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "guarded above: the value is whole, non-negative, and below 2^64 here"
    )]
    fn as_u64(&self) -> Option<u64> {
        match self {
            Self::Unsigned(n) => Some(*n),
            Self::Signed(n) => u64::try_from(*n).ok(),
            Self::Float(f) => {
                (f.fract() == 0.0 && *f >= 0.0 && *f < U64_LIMIT).then_some(*f as u64)
            }
            Self::Text(s) => s.trim().parse().ok(),
            Self::Bool(_) => None,
        }
    }

    /// Reads the scalar as a signed integer, or `None` if it does not denote one.
    #[allow(
        clippy::cast_possible_truncation,
        reason = "guarded above: the value is whole and within ±2^63 here"
    )]
    fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Unsigned(n) => i64::try_from(*n).ok(),
            Self::Signed(n) => Some(*n),
            Self::Float(f) => {
                (f.fract() == 0.0 && *f >= -I64_LIMIT && *f < I64_LIMIT).then_some(*f as i64)
            }
            Self::Text(s) => s.trim().parse().ok(),
            Self::Bool(_) => None,
        }
    }

    /// Reads the scalar as text, or `None` if it is blank.
    ///
    /// Numbers stringify, because a headend that answers `"category_id": 1` means the same
    /// thing as one that answers `"category_id": "1"`. Empty and whitespace-only text is
    /// Xtream's other spelling of `null`, so it reads as absent.
    fn as_text(&self) -> Option<String> {
        let text = match self {
            Self::Text(s) => s.trim().to_owned(),
            Self::Unsigned(n) => n.to_string(),
            Self::Signed(n) => n.to_string(),
            Self::Float(f) => f.to_string(),
            Self::Bool(_) => return None,
        };
        (!text.is_empty()).then_some(text)
    }

    /// Reads the scalar as a flag, or `None` if it does not denote one.
    ///
    /// Covers every truth encoding observed in the wild: `true`, `1`, `"1"`, `"true"`, and
    /// their negations. Unrecognized text reads as absent rather than silently false, so a
    /// caller can tell "the headend said no" from "the headend said something we can't
    /// read" — the distinction [`UserInfo::auth`] depends on.
    fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            Self::Unsigned(n) => Some(*n != 0),
            Self::Signed(n) => Some(*n != 0),
            Self::Float(f) => Some(*f != 0.0),
            Self::Text(s) => match s.trim().to_ascii_lowercase().as_str() {
                "1" | "true" | "yes" | "on" => Some(true),
                "0" | "false" | "no" | "off" => Some(false),
                _ => None,
            },
        }
    }
}

/// Reads `null`, a number, or a number-as-string as an unsigned integer.
pub(crate) fn de_flex_opt_u64<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::<Scalar>::deserialize(deserializer)?.and_then(|s| s.as_u64()))
}

/// Reads `null`, a number, or a number-as-string as a signed integer.
pub(crate) fn de_flex_opt_i64<'de, D>(deserializer: D) -> Result<Option<i64>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::<Scalar>::deserialize(deserializer)?.and_then(|s| s.as_i64()))
}

/// Reads `null`, `""`, text, or a number as optional text (blank reads as absent).
pub(crate) fn de_flex_opt_string<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::<Scalar>::deserialize(deserializer)?.and_then(|s| s.as_text()))
}

/// Reads `null`, `0`/`1`, `"0"`/`"1"`, or `true`/`false` as an optional flag.
pub(crate) fn de_flex_opt_bool<'de, D>(deserializer: D) -> Result<Option<bool>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::<Scalar>::deserialize(deserializer)?.and_then(|s| s.as_bool()))
}

/// Reads an optional nested object, tolerating PHP's empty-array-for-empty-object.
///
/// The quirk that makes this necessary: PHP cannot distinguish an empty associative array
/// from an empty list, so `json_encode([])` emits `[]` where the panel meant `{}`. A plain
/// `Option<T>` would fail to deserialize that and — because the failure surfaces at the row
/// boundary — cost the caller the entire episode over an absent `info` block. An array here
/// means "nothing to report", which is exactly what `null` means.
pub(crate) fn de_flex_opt_object<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    let Some(value) = Option::<Value>::deserialize(deserializer)? else {
        return Ok(None);
    };
    match value {
        Value::Object(_) => T::deserialize(value)
            .map(Some)
            .map_err(serde::de::Error::custom),
        Value::Array(_) | Value::Null => Ok(None),
        other => Err(serde::de::Error::custom(format!(
            "expected a nested object, found {}",
            json_type_of(&other)
        ))),
    }
}

/// The `player_api.php` handshake response.
#[derive(Deserialize)]
pub(crate) struct AuthEnvelope {
    /// Absent when the response is JSON but not a handshake — an envelope failure.
    #[serde(default)]
    pub(crate) user_info: Option<UserInfo>,
}

/// The account block of the handshake.
///
/// Deliberately does **not** declare `username` or `password`: real headends echo both back
/// in this object, and the surest way to keep a credential out of the crate is to give it
/// nowhere to be deserialized into (§12).
#[derive(Deserialize)]
pub(crate) struct UserInfo {
    /// `1`/`0`/`true`/`false` — whether the credentials were accepted at all.
    #[serde(default, deserialize_with = "de_flex_opt_bool")]
    pub(crate) auth: Option<bool>,
    /// `Active`, `Expired`, `Banned`, or a vendor's own wording.
    #[serde(default, deserialize_with = "de_flex_opt_string")]
    pub(crate) status: Option<String>,
    /// Subscription expiry as Unix seconds, usually spelled as a string.
    #[serde(default, deserialize_with = "de_flex_opt_i64")]
    pub(crate) exp_date: Option<i64>,
    /// Concurrent-connection allowance.
    #[serde(default, deserialize_with = "de_flex_opt_u64")]
    pub(crate) max_connections: Option<u64>,
    /// Connections currently in use.
    #[serde(default, deserialize_with = "de_flex_opt_u64")]
    pub(crate) active_cons: Option<u64>,
}

/// One row of `get_{live,vod,series}_categories`.
#[derive(Deserialize)]
pub(crate) struct CategoryDto {
    #[serde(default, deserialize_with = "de_flex_opt_string")]
    pub(crate) category_id: Option<String>,
    #[serde(default, deserialize_with = "de_flex_opt_string")]
    pub(crate) category_name: Option<String>,
}

/// One row of `get_live_streams` or `get_vod_streams`.
///
/// One DTO covers both because the fields the catalog mapping needs are the same; the only
/// difference is that live rows carry no `container_extension` (hence `crate::urls`'
/// per-kind default).
#[derive(Deserialize)]
pub(crate) struct StreamDto {
    #[serde(default, deserialize_with = "de_flex_opt_u64")]
    pub(crate) stream_id: Option<u64>,
    #[serde(default, deserialize_with = "de_flex_opt_string")]
    pub(crate) name: Option<String>,
    #[serde(default, deserialize_with = "de_flex_opt_string")]
    pub(crate) stream_icon: Option<String>,
    #[serde(default, deserialize_with = "de_flex_opt_string")]
    pub(crate) epg_channel_id: Option<String>,
    #[serde(default, deserialize_with = "de_flex_opt_string")]
    pub(crate) category_id: Option<String>,
    #[serde(default, deserialize_with = "de_flex_opt_string")]
    pub(crate) container_extension: Option<String>,
}

/// One row of `get_series`.
#[derive(Deserialize)]
pub(crate) struct SeriesDto {
    #[serde(default, deserialize_with = "de_flex_opt_u64")]
    pub(crate) series_id: Option<u64>,
    #[serde(default, deserialize_with = "de_flex_opt_string")]
    pub(crate) name: Option<String>,
    #[serde(default, deserialize_with = "de_flex_opt_string")]
    pub(crate) cover: Option<String>,
    #[serde(default, deserialize_with = "de_flex_opt_string")]
    pub(crate) category_id: Option<String>,
}

/// The `get_series_info` response.
#[derive(Deserialize)]
pub(crate) struct SeriesInfoDto {
    #[serde(default, deserialize_with = "de_flex_opt_object")]
    pub(crate) info: Option<SeriesMetaDto>,
    /// Episodes bucketed by season. See [`de_episodes`] for the two shapes this arrives in.
    #[serde(default, deserialize_with = "de_episodes")]
    pub(crate) episodes: Vec<SeasonBucket>,
}

/// The `info` block of `get_series_info` — the series' own metadata.
#[derive(Deserialize)]
pub(crate) struct SeriesMetaDto {
    #[serde(default, deserialize_with = "de_flex_opt_string")]
    pub(crate) name: Option<String>,
    #[serde(default, deserialize_with = "de_flex_opt_string")]
    pub(crate) cover: Option<String>,
}

/// Episodes that arrived under one season key.
pub(crate) struct SeasonBucket {
    /// The season number from the map key, when the response used the keyed shape.
    pub(crate) season: Option<u32>,
    /// Episode rows, still unparsed so that one bad episode is a skip, not a failed season.
    pub(crate) rows: Vec<Value>,
}

/// One episode of `get_series_info`.
#[derive(Deserialize)]
pub(crate) struct EpisodeDto {
    #[serde(default, deserialize_with = "de_flex_opt_u64")]
    pub(crate) id: Option<u64>,
    #[serde(default, deserialize_with = "de_flex_opt_u64")]
    pub(crate) episode_num: Option<u64>,
    #[serde(default, deserialize_with = "de_flex_opt_u64")]
    pub(crate) season: Option<u64>,
    #[serde(default, deserialize_with = "de_flex_opt_string")]
    pub(crate) title: Option<String>,
    #[serde(default, deserialize_with = "de_flex_opt_string")]
    pub(crate) container_extension: Option<String>,
    #[serde(default, deserialize_with = "de_flex_opt_object")]
    pub(crate) info: Option<EpisodeInfoDto>,
}

/// The per-episode `info` block; only its artwork matters to the catalog.
#[derive(Deserialize)]
pub(crate) struct EpisodeInfoDto {
    #[serde(default, deserialize_with = "de_flex_opt_string")]
    pub(crate) movie_image: Option<String>,
}

/// Reads `episodes` in either shape real headends return.
///
/// The documented shape is an **object keyed by season number as a string**
/// (`{"1": [...], "2": [...]}`), but panels that serialize a PHP array with contiguous
/// integer keys emit a **JSON array** instead (`[[...], [...]]`) — the same data, reshaped
/// by a language quirk. Both are accepted; `null` and an absent field read as no episodes.
/// Anything else is an envelope failure and surfaces as a deserialization error.
///
/// Season numbers survive only in the keyed shape, so `season` is `None` for the array
/// shape and `crate::series` falls back to each episode's own `season` field.
fn de_episodes<'de, D>(deserializer: D) -> Result<Vec<SeasonBucket>, D::Error>
where
    D: Deserializer<'de>,
{
    let Some(value) = Option::<Value>::deserialize(deserializer)? else {
        return Ok(Vec::new());
    };
    match value {
        Value::Object(map) => Ok(map
            .into_iter()
            .map(|(key, rows)| SeasonBucket {
                season: key.trim().parse().ok(),
                rows: into_rows(rows),
            })
            .collect()),
        Value::Array(seasons) => Ok(seasons
            .into_iter()
            .map(|rows| SeasonBucket {
                season: None,
                rows: into_rows(rows),
            })
            .collect()),
        Value::Null => Ok(Vec::new()),
        other => Err(serde::de::Error::custom(format!(
            "`episodes` must be an object or an array, found {}",
            json_type_of(&other)
        ))),
    }
}

/// A season's payload as a row list; a non-array payload contributes no episodes.
fn into_rows(value: Value) -> Vec<Value> {
    match value {
        Value::Array(rows) => rows,
        _ => Vec::new(),
    }
}

/// The JSON type name of `value`, for error messages that never echo the payload.
pub(crate) fn json_type_of(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    /// A probe struct per deserializer, so each mapping decision is asserted directly on
    /// the JSON spelling that provoked it.
    #[derive(Deserialize)]
    struct ProbeU64 {
        #[serde(default, deserialize_with = "de_flex_opt_u64")]
        field: Option<u64>,
    }
    #[derive(Deserialize)]
    struct ProbeI64 {
        #[serde(default, deserialize_with = "de_flex_opt_i64")]
        field: Option<i64>,
    }
    #[derive(Deserialize)]
    struct ProbeString {
        #[serde(default, deserialize_with = "de_flex_opt_string")]
        field: Option<String>,
    }
    #[derive(Deserialize)]
    struct ProbeBool {
        #[serde(default, deserialize_with = "de_flex_opt_bool")]
        field: Option<bool>,
    }

    fn u64_of(json: &str) -> Option<u64> {
        serde_json::from_str::<ProbeU64>(json).unwrap().field
    }
    fn i64_of(json: &str) -> Option<i64> {
        serde_json::from_str::<ProbeI64>(json).unwrap().field
    }
    fn string_of(json: &str) -> Option<String> {
        serde_json::from_str::<ProbeString>(json).unwrap().field
    }
    fn bool_of(json: &str) -> Option<bool> {
        serde_json::from_str::<ProbeBool>(json).unwrap().field
    }

    #[test]
    fn integers_read_however_they_are_spelled() {
        // The canonical Xtream quirk: the same id, four ways.
        assert_eq!(u64_of(r#"{"field": 123}"#), Some(123));
        assert_eq!(u64_of(r#"{"field": "123"}"#), Some(123));
        assert_eq!(u64_of(r#"{"field": " 123 "}"#), Some(123));
        assert_eq!(u64_of(r#"{"field": 123.0}"#), Some(123));
    }

    #[test]
    fn absent_integers_read_as_none_in_every_spelling() {
        assert_eq!(u64_of("{}"), None, "missing field");
        assert_eq!(u64_of(r#"{"field": null}"#), None, "explicit null");
        assert_eq!(u64_of(r#"{"field": ""}"#), None, "empty-string-for-null");
        assert_eq!(u64_of(r#"{"field": "   "}"#), None, "whitespace-for-null");
    }

    #[test]
    fn unreadable_integers_read_as_none_rather_than_a_plausible_lie() {
        assert_eq!(u64_of(r#"{"field": "abc"}"#), None, "not a number");
        assert_eq!(u64_of(r#"{"field": -5}"#), None, "negative is not an id");
        assert_eq!(u64_of(r#"{"field": "-5"}"#), None);
        assert_eq!(u64_of(r#"{"field": 1.5}"#), None, "must not truncate to 1");
        assert_eq!(u64_of(r#"{"field": true}"#), None, "a flag is not an id");
    }

    #[test]
    fn out_of_range_floats_read_as_none_rather_than_saturating() {
        // `as` saturates, so an unguarded cast would turn 1e30 into `u64::MAX` — a number
        // the headend never sent. Absent is the honest answer.
        assert_eq!(u64_of(r#"{"field": 1e30}"#), None);
        assert_eq!(i64_of(r#"{"field": 1e30}"#), None);
        assert_eq!(i64_of(r#"{"field": -1e30}"#), None);
    }

    #[test]
    fn signed_integers_accept_negatives_but_not_fractions() {
        assert_eq!(i64_of(r#"{"field": "1735689600"}"#), Some(1_735_689_600));
        assert_eq!(i64_of(r#"{"field": -1}"#), Some(-1));
        assert_eq!(i64_of(r#"{"field": "-1"}"#), Some(-1));
        assert_eq!(i64_of(r#"{"field": 1.5}"#), None);
    }

    #[test]
    fn strings_absorb_numbers_and_treat_blank_as_null() {
        assert_eq!(string_of(r#"{"field": "News"}"#).as_deref(), Some("News"));
        assert_eq!(string_of(r#"{"field": 1}"#).as_deref(), Some("1"));
        assert_eq!(string_of(r#"{"field": "  x  "}"#).as_deref(), Some("x"));
        assert_eq!(string_of(r#"{"field": ""}"#), None);
        assert_eq!(string_of(r#"{"field": "   "}"#), None);
        assert_eq!(string_of(r#"{"field": null}"#), None);
        assert_eq!(string_of("{}"), None);
    }

    #[test]
    fn flags_read_every_truth_encoding_seen_in_the_wild() {
        for json in [
            r#"{"field": 1}"#,
            r#"{"field": "1"}"#,
            r#"{"field": true}"#,
            r#"{"field": "true"}"#,
            r#"{"field": "YES"}"#,
        ] {
            assert_eq!(bool_of(json), Some(true), "{json} should read as true");
        }
        for json in [
            r#"{"field": 0}"#,
            r#"{"field": "0"}"#,
            r#"{"field": false}"#,
            r#"{"field": "false"}"#,
            r#"{"field": "no"}"#,
        ] {
            assert_eq!(bool_of(json), Some(false), "{json} should read as false");
        }
    }

    #[test]
    fn unreadable_flags_are_absent_not_false() {
        // "we can't read this" must stay distinguishable from "the headend said no",
        // because `auth` treats them differently.
        assert_eq!(bool_of(r#"{"field": "maybe"}"#), None);
        assert_eq!(bool_of(r#"{"field": ""}"#), None);
        assert_eq!(bool_of("{}"), None);
    }

    #[test]
    fn a_container_where_a_scalar_belongs_fails_the_row() {
        // Not a tolerated spelling: the mapper turns this error into a counted skip.
        assert!(serde_json::from_str::<ProbeU64>(r#"{"field": [1]}"#).is_err());
        assert!(serde_json::from_str::<ProbeString>(r#"{"field": {"a": 1}}"#).is_err());
    }

    #[test]
    fn user_info_cannot_absorb_the_echoed_password() {
        // Real headends mirror the credentials back; the DTO must drop them (§12).
        let json = r#"{
            "user_info": {
                "username": "alice",
                "password": "s3cr3t-passphrase",
                "auth": 1,
                "status": "Active",
                "exp_date": "1735689600",
                "max_connections": "2",
                "active_cons": 0
            }
        }"#;
        let envelope: AuthEnvelope = serde_json::from_str(json).unwrap();
        let info = envelope.user_info.expect("user_info must be read");
        assert_eq!(info.auth, Some(true));
        assert_eq!(info.status.as_deref(), Some("Active"));
        assert_eq!(info.exp_date, Some(1_735_689_600));
        assert_eq!(info.max_connections, Some(2));
        assert_eq!(info.active_cons, Some(0));
        // The password had nowhere to land: the struct has no field for it, and the whole
        // module is private, so it cannot be reached from outside the crate either.
    }

    #[test]
    fn episodes_read_from_the_keyed_object_shape() {
        let json = r#"{"episodes": {"1": [{"id": "10"}], "2": [{"id": "20"}, {"id": "21"}]}}"#;
        let info: SeriesInfoDto = serde_json::from_str(json).unwrap();
        let mut buckets = info.episodes;
        buckets.sort_by_key(|b| b.season);
        assert_eq!(buckets.len(), 2);
        assert_eq!(buckets[0].season, Some(1));
        assert_eq!(buckets[0].rows.len(), 1);
        assert_eq!(buckets[1].season, Some(2));
        assert_eq!(buckets[1].rows.len(), 2);
    }

    #[test]
    fn episodes_read_from_the_array_shape_without_season_keys() {
        // A PHP array with contiguous keys serializes as JSON array; same data, no keys.
        let json = r#"{"episodes": [[{"id": "10"}], [{"id": "20"}]]}"#;
        let info: SeriesInfoDto = serde_json::from_str(json).unwrap();
        assert_eq!(info.episodes.len(), 2);
        assert!(
            info.episodes.iter().all(|b| b.season.is_none()),
            "the array shape carries no season keys"
        );
        assert_eq!(info.episodes[0].rows.len(), 1);
    }

    #[test]
    fn absent_episodes_are_no_episodes_not_an_error() {
        for json in ["{}", r#"{"episodes": null}"#, r#"{"episodes": {}}"#] {
            let info: SeriesInfoDto = serde_json::from_str(json).unwrap();
            assert!(info.episodes.is_empty(), "{json} should yield no episodes");
        }
    }

    #[test]
    fn an_episodes_field_of_the_wrong_type_is_an_envelope_failure() {
        // `.err()` rather than `.unwrap_err()`: the latter needs `Debug` on the DTO, and
        // these types stay Debug-free so a response can never be rendered wholesale.
        let err = serde_json::from_str::<SeriesInfoDto>(r#"{"episodes": "nope"}"#)
            .err()
            .expect("a string `episodes` must not deserialize");
        assert!(
            err.to_string().contains("must be an object or an array"),
            "unexpected message: {err}"
        );
    }

    #[test]
    fn an_empty_php_array_reads_as_an_absent_nested_object() {
        // `json_encode([])` in PHP emits `[]` where the panel meant `{}`. Without this
        // tolerance the row fails to deserialize and the whole episode is lost.
        let info: SeriesInfoDto =
            serde_json::from_str(r#"{"info": [], "episodes": {"1": [{"id": 1, "info": []}]}}"#)
                .unwrap();
        assert!(info.info.is_none(), "`info: []` must read as absent");

        let episode: EpisodeDto = serde_json::from_value(info.episodes[0].rows[0].clone()).unwrap();
        assert!(episode.info.is_none());
        assert_eq!(episode.id, Some(1), "the episode itself must survive");
    }

    #[test]
    fn a_populated_nested_object_still_reads_normally() {
        let info: SeriesInfoDto =
            serde_json::from_str(r#"{"info": {"name": "Show", "cover": ""}}"#).unwrap();
        let meta = info.info.expect("a real object must be read");
        assert_eq!(meta.name.as_deref(), Some("Show"));
        assert_eq!(meta.cover, None, "blank cover is absent, not empty");
    }

    #[test]
    fn a_scalar_where_a_nested_object_belongs_fails_the_row() {
        assert!(serde_json::from_str::<SeriesInfoDto>(r#"{"info": "nope"}"#).is_err());
    }

    #[test]
    fn a_season_whose_payload_is_not_a_list_contributes_no_episodes() {
        let json = r#"{"episodes": {"1": "broken", "2": [{"id": 1}]}}"#;
        let info: SeriesInfoDto = serde_json::from_str(json).unwrap();
        let total: usize = info.episodes.iter().map(|b| b.rows.len()).sum();
        assert_eq!(total, 1, "the broken season must not fail the good one");
    }
}
