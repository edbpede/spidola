// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import core_api

/// The narrow core surface the **sources** slice needs (add / list / manage / import). Feature
/// code depends on this protocol, never the concrete `SpidolaCore`, so its view models are unit
/// tested against a fake (TECH_SPEC §10). `SpidolaCore` is the sole production conformer.
public protocol SourcesAccess: Sendable {
  func sources() async throws -> [Source]
  func addM3uUrl(name: String, url: String, userAgent: String?, acceptInvalidTls: Bool)
    async throws -> Source
  func addM3uFile(name: String) async throws -> Source
  func rename(id: Int64, name: String) async throws
  func setEnabled(id: Int64, enabled: Bool) async throws
  func setAutoRefresh(id: Int64, secs: UInt32?) async throws
  func deleteSource(id: Int64) async throws
  /// Fetches (over HTTP) and imports an M3U-by-URL source, streaming progress then one terminal
  /// event. Cancelling the consuming task cancels the core task at its next batch boundary.
  func importURL(id: Int64) -> AsyncStream<ImportEvent>
  /// Imports an M3U-from-file source from already-read `content` (picked file or pasted text),
  /// streaming progress then one terminal event.
  func importContent(id: Int64, content: String) -> AsyncStream<ImportEvent>
}

/// The narrow core surface the **browse** slice needs: the source → type → category → channel
/// drill-down (paged by contract), plus the per-channel context actions (favorite, hide) and the
/// play-time recents record.
public protocol BrowseAccess: Sendable {
  func sources() async throws -> [Source]
  func kinds(sourceId: Int64) async throws -> [MediaKind]
  func groups(sourceId: Int64, kind: MediaKind, offset: UInt32, limit: UInt32) async throws
    -> BrowseGroupPage
  func channelsInGroup(
    sourceId: Int64, kind: MediaKind, group: String?, offset: UInt32, limit: UInt32
  ) async throws -> ChannelPage
  func isFavorite(sourceId: Int64, identity: Int64) async throws -> Bool
  func setFavorite(sourceId: Int64, identity: Int64, favorite: Bool) async throws
  /// The stable identities of a source's favorites, so a channel list can mark them in one query
  /// rather than one `isFavorite` call per row.
  func favoriteIdentities(sourceId: Int64) async throws -> [Int64]
  func isHidden(sourceId: Int64, identity: Int64) async throws -> Bool
  func setHidden(sourceId: Int64, identity: Int64, hidden: Bool) async throws
  func recordRecent(_ channel: PlayableChannel) async throws
}

/// The narrow core surface the **search** slice needs: the ranked, paged query plus the source
/// list for the source filter.
public protocol SearchAccess: Sendable {
  func sources() async throws -> [Source]
  func search(query: String, sourceId: Int64?, kind: MediaKind?, offset: UInt32, limit: UInt32)
    async throws -> SearchPage
}

/// The narrow core surface the **settings** slice needs: the one-shot snapshot the root screen
/// renders every row from, one setter per closed-set setting, the recently-watched off-switch and
/// its clear action, the log buffer, and the startup handshake the diagnostics screen reports.
///
/// Three deliberate absences, each of which would otherwise look like an oversight:
///
/// - **The recents off-switch is not a settings setter.** `settingsSnapshot()` *reports* it, but
///   `RecentsService` stays its only writer because that service is what enforces it — so the
///   toggle routes to `setRecentsEnabled` (shared with `HomeAccess`) and the settings vocabulary
///   never writes it. Mirrors the ownership the core states in `services/settings.rs`.
/// - **No EPG window.** The core's vocabulary carries it because PRD §6.9 lists it, but EPG ingest
///   is Phase 8; a setting that changed nothing a viewer could see would be a UX bug, so the slice
///   does not ask for it and this protocol does not offer it. It lands with the EPG screens.
/// - **No per-source engine override.** This surface owns the *global* default player; a per-source
///   choice belongs to the screen about that source, not to a list of app-wide preferences.
public protocol SettingsAccess: Sendable {
  /// Every setting resolved to a value — stored where the user set one, core default otherwise —
  /// so a screen renders each row's current value from a single read taken at one instant.
  func settingsSnapshot() async throws -> AppSettings

  /// Sets the global default engine, or clears it with `nil` to follow the platform default. The
  /// key is opaque to the core; `PlayerContract` owns the spellings (TECH_SPEC §8).
  func setDefaultEngine(_ engine: String?) async throws
  func setBuffering(_ profile: BufferingProfile) async throws
  func setSubtitleSize(_ size: SubtitleSize) async throws
  func setSubtitleBackground(_ background: SubtitleBackground) async throws
  /// Sets the UI language as a BCP-47 tag, or clears it with `nil` to follow the system language.
  func setLanguage(_ tag: String?) async throws
  func setDensity(_ density: InterfaceDensity) async throws
  func setRecentsRetentionDays(_ days: UInt32) async throws
  func setImageCacheMaxMb(_ megabytes: UInt32) async throws
  /// Persists the level *and* applies it to the running filter in one call, so the stored value
  /// and the live filter cannot disagree (core `SettingsService`).
  func setLogLevel(_ level: LogLevel) async throws

  /// The recently-watched off-switch and its history, owned by the core's `RecentsService`.
  func recentsEnabled() async throws -> Bool
  func setRecentsEnabled(_ enabled: Bool) async throws
  func clearRecents() async throws

  /// The in-memory log ring, oldest first. On tvOS this *is* "export logs" (PRD §6.9): there is no
  /// user-visible file system and no share sheet worth the name, so the diagnostics screen shows
  /// the lines on screen rather than writing a file nobody could reach.
  func exportLogs() -> [String]
  /// The core / schema / boundary versions, reported on the diagnostics screen.
  func handshake() -> Handshake
}

/// The narrow core surface the **home** screen needs: the favorites row, the recents row with its
/// off-switch, and the enabled source list to browse into.
public protocol HomeAccess: Sendable {
  func sources() async throws -> [Source]
  func favoriteChannels(offset: UInt32, limit: UInt32) async throws -> ChannelPage
  func recents(limit: UInt32) async throws -> [Recent]
  func recentsEnabled() async throws -> Bool
  func setRecentsEnabled(_ enabled: Bool) async throws
  func clearRecents() async throws
  func recordRecent(_ channel: PlayableChannel) async throws
}
