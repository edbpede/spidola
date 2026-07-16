// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The single audited point of stream-URL credential embedding (TECH_SPEC §4.3, §12).
//!
//! # The audit
//!
//! Xtream is the one protocol Spidola speaks that puts the account password *in the URL*
//! (`{server}/{live|movie|series}/{username}/{password}/{id}.{ext}`) rather than in a
//! header. Every URL in the crate that carries a credential is built by exactly one
//! function here — [`Endpoint::player_api`] for API calls, [`Endpoint::resolve_stream`] for
//! playback — and both obey the same four rules, which are what an auditor should check:
//!
//! 1. **The password is borrowed, never stored.** It arrives as `&Secret` and is read with
//!    `Secret::expose` on the single line that writes it into the URL. No field, struct, or
//!    local in this crate outlives that expression holding a credential.
//! 2. **Encoding is by construction, not by concatenation.** Segments are written through
//!    `url`'s path-segment writer, which percent-encodes the reserved set itself. There is
//!    no `format!` of a credential into a path anywhere in the crate — a password
//!    containing `/`, `%`, `?`, or `#` cannot restructure the URL. This is pinned by
//!    `passwords_with_url_metacharacters_are_percent_encoded`.
//! 3. **The output redacts itself.** Both constructors return a newtype
//!    ([`CredentialUrl`], [`ResolvedStream`]) whose `Debug` is redacted, so a credential
//!    URL cannot reach the log stream or a `{:?}` by accident — the same posture
//!    `core_model::secret::Secret` takes, for the same reason. Reaching the raw URL is an
//!    explicit, greppable act.
//! 4. **Credential URLs are never persisted.** See [`StreamRef`] below.
//!
//! # Why the catalog stores a credential-free locator
//!
//! A channel's `locator` is persisted in SQLite, snapshotted again into playback history,
//! and handed across the FFI to the shells. §12 is unambiguous that credentials never
//! persist in SQLite, so the *playable* Xtream URL cannot be the stored one. Instead the
//! catalog stores the credential-free [`StreamRef::to_catalog_locator`] form
//! (`{server}/{live|movie|series}/{id}.{ext}`), and the credentials are re-embedded at zap
//! time by [`Endpoint::resolve_stream`], reading the password from the host-secrets
//! callback at that moment. The round trip back out of storage is
//! [`StreamRef::from_catalog_locator`], and that it is lossless is a pinned law
//! (`catalog_locators_round_trip`).
//!
//! This also buys identity stability for free: because the stored locator holds no
//! password, rotating an account's password does not perturb `channel_identity`, so
//! favorites and hidden flags survive it (§4.4).

use core_model::channel::MediaKind;
use core_model::locator::StreamLocator;
use core_model::secret::Secret;
use url::Url;

use crate::error::{XtreamError, XtreamResult};

/// Longest plausible container extension (`m3u8` is 4; the cap is slack, not a guess).
const MAX_EXTENSION_LEN: usize = 8;

/// The default container for live streams, which Xtream does not label.
pub const DEFAULT_LIVE_EXTENSION: &str = "ts";

/// The default container for VOD/series rows whose `container_extension` is absent.
pub const DEFAULT_VOD_EXTENSION: &str = "mp4";

/// A URL with an account password embedded in it.
///
/// Exists so that a credential URL cannot be logged, formatted, or persisted by accident:
/// `Debug` is redacted and there is no `Display`. Mirrors `Secret`'s posture, and like
/// `Secret` it deliberately does not derive `Clone`.
pub struct CredentialUrl {
    inner: Url,
}

impl CredentialUrl {
    /// Borrows the raw URL for a single use — issuing the request.
    ///
    /// The returned slice must not be copied into a longer-lived owner, logged, or
    /// persisted. This is the one sanctioned way to read it.
    #[must_use]
    pub fn expose(&self) -> &str {
        self.inner.as_str()
    }
}

impl std::fmt::Debug for CredentialUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CredentialUrl([REDACTED])")
    }
}

/// A playable Xtream stream URL, with the account credentials embedded.
///
/// The payload is a validated [`StreamLocator`] — it parsed, so "parse, don't validate"
/// holds — but it is wrapped rather than returned bare because a bare `StreamLocator`
/// renders its URL in `Debug` and `Display` and serializes itself, any of which would put
/// the password in a log line or a database. Unwrapping via [`Self::into_locator`] is
/// therefore a deliberate, greppable act, and the resulting locator must be handed to the
/// player and dropped — never stored (§12).
pub struct ResolvedStream {
    inner: StreamLocator,
}

impl ResolvedStream {
    /// Consumes the wrapper, yielding the playable locator.
    ///
    /// Call this at the point the URL is handed to a player engine. The value must not be
    /// persisted, logged, or snapshotted into playback history.
    #[must_use]
    pub fn into_locator(self) -> StreamLocator {
        self.inner
    }
}

impl std::fmt::Debug for ResolvedStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ResolvedStream([REDACTED])")
    }
}

/// A credential-free reference to one Xtream stream: everything needed to rebuild its URL
/// except the credentials themselves.
///
/// This is what survives into the catalog (as [`Self::to_catalog_locator`]) and what comes
/// back out of it at zap time (via [`Self::from_catalog_locator`]) to be resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamRef {
    kind: MediaKind,
    stream_id: u64,
    extension: String,
}

impl StreamRef {
    /// Builds a reference, rejecting an absent id or an implausible container extension.
    ///
    /// `extension` is `None` for live streams (Xtream does not label them) and the row's
    /// `container_extension` otherwise; the defaults are [`DEFAULT_LIVE_EXTENSION`] and
    /// [`DEFAULT_VOD_EXTENSION`]. A non-alphanumeric or overlong extension yields `None` so
    /// the caller records it as a skip — the URL builder would encode it harmlessly, but a
    /// row whose container is gibberish has nothing playable behind it either way.
    #[must_use]
    pub fn new(kind: MediaKind, stream_id: u64, extension: Option<&str>) -> Option<Self> {
        if stream_id == 0 {
            return None;
        }
        let extension = match extension.map(str::trim) {
            None | Some("") => match kind {
                MediaKind::Live => DEFAULT_LIVE_EXTENSION,
                MediaKind::Movie | MediaKind::SeriesEpisode => DEFAULT_VOD_EXTENSION,
            },
            Some(raw) => raw,
        };
        if !is_plausible_extension(extension) {
            return None;
        }
        Some(Self {
            kind,
            stream_id,
            extension: extension.to_ascii_lowercase(),
        })
    }

    /// What this stream plays.
    #[must_use]
    pub fn kind(&self) -> MediaKind {
        self.kind
    }

    /// The headend's id for this stream.
    #[must_use]
    pub fn stream_id(&self) -> u64 {
        self.stream_id
    }

    /// The container extension, lowercased.
    #[must_use]
    pub fn extension(&self) -> &str {
        &self.extension
    }

    /// The **credential-free** locator to persist: `{server}/{kind}/{id}.{ext}`.
    ///
    /// Deliberately not playable on its own — the credential segments are absent. It is a
    /// durable reference that [`Endpoint::resolve_stream`] turns into a playable URL at zap
    /// time, which is what keeps the password out of SQLite (§12).
    ///
    /// # Errors
    /// Returns [`XtreamError::InvalidServer`] if `server` cannot host a path, or
    /// [`XtreamError::Malformed`] if the assembled URL somehow fails to parse as a locator.
    pub fn to_catalog_locator(&self, server: &Url) -> XtreamResult<StreamLocator> {
        let mut url = server.clone();
        push_path(&mut url, &[self.path_kind(), &self.file_name()])?;
        StreamLocator::parse(url.as_str()).map_err(|e| XtreamError::Malformed {
            detail: format!("built an unparseable stream URL: {e}"),
        })
    }

    /// Recovers a reference from a locator previously produced by
    /// [`Self::to_catalog_locator`], returning `None` if it is not that shape.
    ///
    /// Reads the last two path segments, so an Xtream account served from a sub-path
    /// (`https://host/panel/live/1.ts`) round-trips as well as one served from the root.
    #[must_use]
    pub fn from_catalog_locator(locator: &StreamLocator) -> Option<Self> {
        let url = Url::parse(locator.as_str()).ok()?;
        let segments: Vec<&str> = url.path_segments()?.collect();
        let [.., kind, file] = segments.as_slice() else {
            return None;
        };
        let kind = kind_from_path(kind)?;
        let (id, extension) = file.rsplit_once('.')?;
        Self::new(kind, id.parse().ok()?, Some(extension))
    }

    /// The path segment Xtream uses for this kind.
    const fn path_kind(&self) -> &'static str {
        match self.kind {
            MediaKind::Live => "live",
            MediaKind::Movie => "movie",
            MediaKind::SeriesEpisode => "series",
        }
    }

    fn file_name(&self) -> String {
        format!("{}.{}", self.stream_id, self.extension)
    }
}

/// The non-secret coordinates of an Xtream account: where it lives and who it is.
///
/// Holds no credential — the password is passed per call as a `&Secret` and borrowed only
/// for the expression that writes it into a URL.
#[derive(Debug, Clone)]
pub struct Endpoint {
    server: Url,
    username: String,
}

impl Endpoint {
    /// Parses an account's server address into a usable API base.
    ///
    /// # Errors
    /// Returns [`XtreamError::InvalidServer`] if the address is not a hierarchical URL (a
    /// `mailto:`-style opaque URL has no path to append to), or if the username is blank.
    pub fn new(server: &StreamLocator, username: &str) -> XtreamResult<Self> {
        let server = Url::parse(server.as_str()).map_err(|e| XtreamError::InvalidServer {
            reason: e.to_string(),
        })?;
        if server.cannot_be_a_base() {
            return Err(XtreamError::InvalidServer {
                reason: "the address has no path to append the API to".to_owned(),
            });
        }
        let username = username.trim();
        if username.is_empty() {
            return Err(XtreamError::InvalidServer {
                reason: "the account has no username".to_owned(),
            });
        }
        Ok(Self {
            server,
            username: username.to_owned(),
        })
    }

    /// The account's server base URL.
    #[must_use]
    pub fn server(&self) -> &Url {
        &self.server
    }

    /// The account username (not a secret).
    #[must_use]
    pub fn username(&self) -> &str {
        &self.username
    }

    /// **Audited credential embedding.** Builds a `player_api.php` URL carrying the
    /// account's credentials as query parameters, plus `params` (typically `action=…`).
    ///
    /// The password is written by `url`'s form-encoding query writer, so no value can break
    /// out of its parameter. The result redacts itself; see the module audit.
    ///
    /// # Errors
    /// Returns [`XtreamError::InvalidServer`] if the base URL cannot take a path.
    pub fn player_api(
        &self,
        password: &Secret,
        params: &[(&str, &str)],
    ) -> XtreamResult<CredentialUrl> {
        let mut url = self.server.clone();
        push_path(&mut url, &["player_api.php"])?;
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("username", &self.username);
            // The one place a password enters an API URL. Borrowed for this expression only.
            query.append_pair("password", password.expose());
            for (key, value) in params {
                query.append_pair(key, value);
            }
        }
        Ok(CredentialUrl { inner: url })
    }

    /// Builds the account-wide XMLTV endpoint used for schedule ingestion.
    ///
    /// The credential handling is identical to [`Self::player_api`]: values are form-encoded,
    /// the result redacts itself, and callers expose it only to `core-fetch` for one request.
    ///
    /// # Errors
    /// Returns [`XtreamError::InvalidServer`] if the base cannot host a path.
    pub fn xmltv(&self, password: &Secret) -> XtreamResult<CredentialUrl> {
        let mut url = self.server.clone();
        push_path(&mut url, &["xmltv.php"])?;
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("username", &self.username);
            query.append_pair("password", password.expose());
        }
        Ok(CredentialUrl { inner: url })
    }

    /// **Audited credential embedding.** Builds the playable stream URL for `stream`:
    /// `{server}/{live|movie|series}/{username}/{password}/{id}.{ext}`.
    ///
    /// The username and password are written by `url`'s path-segment writer, which
    /// percent-encodes the reserved set, so a password containing `/` or `?` cannot
    /// restructure the path. The result redacts itself and must not be persisted; see the
    /// module audit.
    ///
    /// # Errors
    /// Returns [`XtreamError::InvalidServer`] if the base URL cannot take a path, or
    /// [`XtreamError::Malformed`] if the assembled URL fails to parse as a locator.
    pub fn resolve_stream(
        &self,
        password: &Secret,
        stream: &StreamRef,
    ) -> XtreamResult<ResolvedStream> {
        let mut url = self.server.clone();
        push_path(
            &mut url,
            &[
                stream.path_kind(),
                &self.username,
                // The one place a password enters a stream URL. Borrowed for this call only.
                password.expose(),
                &stream.file_name(),
            ],
        )?;
        let inner = StreamLocator::parse(url.as_str()).map_err(|e| XtreamError::Malformed {
            detail: format!("built an unparseable stream URL: {e}"),
        })?;
        Ok(ResolvedStream { inner })
    }
}

/// Appends `segments` to `url`'s path, percent-encoding each one.
///
/// `pop_if_empty` absorbs a trailing slash on the base (`http://host:8080/` and
/// `http://host:8080` must produce the same result), and the writer encodes every segment,
/// which is the property the credential audit rests on.
fn push_path(url: &mut Url, segments: &[&str]) -> XtreamResult<()> {
    let mut path = url
        .path_segments_mut()
        .map_err(|()| XtreamError::InvalidServer {
            reason: "the address has no path to append the API to".to_owned(),
        })?;
    path.pop_if_empty().extend(segments);
    Ok(())
}

/// The [`MediaKind`] behind an Xtream path segment.
fn kind_from_path(segment: &str) -> Option<MediaKind> {
    match segment {
        "live" => Some(MediaKind::Live),
        "movie" => Some(MediaKind::Movie),
        "series" => Some(MediaKind::SeriesEpisode),
        _ => None,
    }
}

/// Whether `candidate` looks like a container extension (`ts`, `m3u8`, `mkv`, …).
fn is_plausible_extension(candidate: &str) -> bool {
    !candidate.is_empty()
        && candidate.len() <= MAX_EXTENSION_LEN
        && candidate.chars().all(|c| c.is_ascii_alphanumeric())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    fn endpoint(server: &str, username: &str) -> Endpoint {
        Endpoint::new(&StreamLocator::parse(server).unwrap(), username).unwrap()
    }

    fn live(id: u64) -> StreamRef {
        StreamRef::new(MediaKind::Live, id, None).unwrap()
    }

    // ---- The audit: encoding -------------------------------------------------------

    #[test]
    fn stream_urls_have_the_documented_shape() {
        let resolved = endpoint("http://panel.example:8080", "alice")
            .resolve_stream(&Secret::new("hunter2"), &live(4242))
            .unwrap();
        assert_eq!(
            resolved.into_locator().as_str(),
            "http://panel.example:8080/live/alice/hunter2/4242.ts"
        );
    }

    #[test]
    fn xmltv_credentials_are_encoded_and_redacted() {
        let url = endpoint("http://panel.example:8080", "alice/name")
            .xmltv(&Secret::new("p&a=s#word"))
            .unwrap();
        assert_eq!(format!("{url:?}"), "CredentialUrl([REDACTED])");
        let parsed = Url::parse(url.expose()).unwrap();
        let query = parsed
            .query_pairs()
            .collect::<std::collections::HashMap<_, _>>();
        assert_eq!(
            query
                .get("username")
                .map(std::convert::AsRef::<str>::as_ref),
            Some("alice/name")
        );
        assert_eq!(
            query
                .get("password")
                .map(std::convert::AsRef::<str>::as_ref),
            Some("p&a=s#word")
        );
    }

    #[test]
    fn passwords_with_url_metacharacters_are_percent_encoded() {
        // The whole audit rests on this: a password full of URL structure must land in one
        // path segment and change nothing about the path's shape.
        let resolved = endpoint("http://panel.example", "user/name")
            .resolve_stream(&Secret::new("p/a?s#s%21 x"), &live(7))
            .unwrap();
        let url = resolved.into_locator();
        let parsed = Url::parse(url.as_str()).unwrap();
        let segments: Vec<&str> = parsed.path_segments().unwrap().collect();
        // Exactly `{kind}/{user}/{pass}/{file}` — the credentials did not restructure it.
        assert_eq!(segments.len(), 4, "password broke out of its segment");
        assert_eq!(segments[0], "live");
        assert_eq!(segments[3], "7.ts");
        // Percent-decoding the credential segments must return exactly what went in. Note
        // `%` itself is escaped to `%25`, so a password cannot forge an escape sequence.
        assert_eq!(decode(segments[1]), "user/name");
        assert_eq!(decode(segments[2]), "p/a?s#s%21 x");
    }

    /// Percent-decodes one path segment, so the encoding assertions test the round trip
    /// rather than restating `url`'s escape table.
    fn decode(input: &str) -> String {
        let bytes = input.as_bytes();
        let mut out = Vec::with_capacity(bytes.len());
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'%' && i + 2 < bytes.len() {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap();
                out.push(u8::from_str_radix(hex, 16).unwrap());
                i += 3;
            } else {
                out.push(bytes[i]);
                i += 1;
            }
        }
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn player_api_urls_carry_credentials_and_action() {
        let url = endpoint("http://panel.example:8080/", "alice")
            .player_api(&Secret::new("p ss&x"), &[("action", "get_live_streams")])
            .unwrap();
        assert_eq!(
            url.expose(),
            "http://panel.example:8080/player_api.php\
             ?username=alice&password=p+ss%26x&action=get_live_streams"
        );
    }

    #[test]
    fn a_base_path_is_preserved_and_a_trailing_slash_absorbed() {
        for server in ["http://host/panel", "http://host/panel/"] {
            let resolved = endpoint(server, "u")
                .resolve_stream(&Secret::new("p"), &live(1))
                .unwrap();
            assert_eq!(
                resolved.into_locator().as_str(),
                "http://host/panel/live/u/p/1.ts",
                "base path handling differed for {server}"
            );
        }
    }

    // ---- The audit: nothing renders a credential -----------------------------------

    #[test]
    fn credential_urls_never_appear_in_debug_output() {
        let endpoint = endpoint("http://panel.example", "alice");
        let password = Secret::new("s3cr3t-passphrase");

        let api = endpoint.player_api(&password, &[]).unwrap();
        let stream = endpoint.resolve_stream(&password, &live(9)).unwrap();

        for rendered in [format!("{api:?}"), format!("{stream:?}")] {
            assert!(
                !rendered.contains("s3cr3t"),
                "a credential URL leaked through Debug: {rendered}"
            );
        }
        assert_eq!(format!("{api:?}"), "CredentialUrl([REDACTED])");
        assert_eq!(format!("{stream:?}"), "ResolvedStream([REDACTED])");
    }

    #[test]
    fn using_a_password_does_not_leave_it_in_the_endpoint() {
        // Audit rule 1: the password is borrowed for the call, never stored. `Endpoint` is
        // `Debug` and long-lived, so if a credential ever lodged in it, it would render
        // here — and this is the type a caller is most likely to log.
        let endpoint = endpoint("http://panel.example", "alice");
        let password = Secret::new("s3cr3t-passphrase");
        let _stream = endpoint.resolve_stream(&password, &live(1)).unwrap();
        let _api = endpoint.player_api(&password, &[]).unwrap();

        let rendered = format!("{endpoint:?}");
        assert!(rendered.contains("alice"), "username is not a secret");
        assert!(
            !rendered.contains("s3cr3t"),
            "the endpoint retained a password after using one: {rendered}"
        );
    }

    // ---- The audit: what gets persisted ---------------------------------------------

    #[test]
    fn catalog_locators_carry_no_credentials() {
        let endpoint = endpoint("http://panel.example:8080", "alice");
        let locator = live(4242).to_catalog_locator(endpoint.server()).unwrap();
        assert_eq!(
            locator.as_str(),
            "http://panel.example:8080/live/4242.ts",
            "the persisted locator must hold neither username nor password"
        );
        // This is the value that reaches SQLite and the FFI (§12): prove it is clean even
        // under the type that *does* render itself.
        let rendered = format!("{locator:?}");
        assert!(!rendered.contains("alice"));
    }

    #[test]
    fn catalog_locators_round_trip() {
        let server = Url::parse("http://panel.example:8080/panel/").unwrap();
        let cases = [
            StreamRef::new(MediaKind::Live, 1, None).unwrap(),
            StreamRef::new(MediaKind::Movie, 99, Some("mkv")).unwrap(),
            StreamRef::new(MediaKind::SeriesEpisode, 12_345, Some("m3u8")).unwrap(),
        ];
        for original in cases {
            let locator = original.to_catalog_locator(&server).unwrap();
            let recovered = StreamRef::from_catalog_locator(&locator)
                .unwrap_or_else(|| panic!("failed to recover {original:?} from catalog locator"));
            assert_eq!(recovered, original, "round trip lost information");
        }
    }

    #[test]
    fn foreign_locators_do_not_masquerade_as_stream_refs() {
        for raw in [
            "http://host/live/nan.ts",   // id is not a number
            "http://host/vod/1.ts",      // not an Xtream path kind
            "http://host/live/1",        // no extension
            "http://host/live/1.!!",     // implausible extension
            "http://host/live/0.ts",     // the "absent id" spelling
            "http://host/playlist.m3u8", // a plain M3U channel
        ] {
            let locator = StreamLocator::parse(raw).unwrap();
            assert!(
                StreamRef::from_catalog_locator(&locator).is_none(),
                "{raw} should not parse as a stream reference"
            );
        }
    }

    // ---- StreamRef construction ------------------------------------------------------

    #[test]
    fn extensions_default_per_kind_and_normalize() {
        assert_eq!(live(1).extension(), DEFAULT_LIVE_EXTENSION);
        let movie = StreamRef::new(MediaKind::Movie, 1, None).unwrap();
        assert_eq!(movie.extension(), DEFAULT_VOD_EXTENSION);
        // Absent, blank and whitespace-only all mean "the headend did not say".
        let blank = StreamRef::new(MediaKind::Movie, 1, Some("   ")).unwrap();
        assert_eq!(blank.extension(), DEFAULT_VOD_EXTENSION);
        // Case is normalized so the same stream never yields two locators.
        let shouty = StreamRef::new(MediaKind::Movie, 1, Some("MKV")).unwrap();
        assert_eq!(shouty.extension(), "mkv");
    }

    #[test]
    fn implausible_extensions_and_absent_ids_are_refused() {
        assert!(StreamRef::new(MediaKind::Movie, 1, Some("mp4?x=1")).is_none());
        assert!(StreamRef::new(MediaKind::Movie, 1, Some("../../etc")).is_none());
        assert!(StreamRef::new(MediaKind::Movie, 1, Some("waytoolongextension")).is_none());
        // Xtream spells "no id" as 0.
        assert!(StreamRef::new(MediaKind::Live, 0, None).is_none());
    }

    // ---- Endpoint validation ---------------------------------------------------------

    #[test]
    fn endpoints_refuse_unusable_accounts() {
        let server = StreamLocator::parse("http://panel.example").unwrap();
        assert!(matches!(
            Endpoint::new(&server, "  "),
            Err(XtreamError::InvalidServer { .. })
        ));
        // `mailto:` parses as a locator but has no path to hang the API off.
        let opaque = StreamLocator::parse("mailto:someone@example.com").unwrap();
        assert!(matches!(
            Endpoint::new(&opaque, "alice"),
            Err(XtreamError::InvalidServer { .. })
        ));
    }
}
