// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import Foundation
import core_api

/// The narrow core surface the **sources** slice needs (add / list / manage / import). Feature
/// code depends on this protocol, never the concrete `SpidolaCore`, so its view models are unit
/// tested against a fake (TECH_SPEC §10). `SpidolaCore` is the sole production conformer.
public protocol SourcesAccess: Sendable {
  func sources() async throws -> [Source]
  func addM3uUrl(name: String, url: String, userAgent: String?, acceptInvalidTls: Bool)
    async throws -> Source
  func addM3uFile(name: String) async throws -> Source
  /// Adds an Xtream Codes account, **verifying it before storing**: a wrong password comes back
  /// as `Unauthorized` from this call rather than as a mystery on the next refresh, which is what
  /// lets the add screen say so while the person is still standing there. No catalog is fetched —
  /// `importURL` does that next, exactly as it does for a playlist URL.
  ///
  /// `password` is in flight to the host secure store and nowhere else: hand it straight to this
  /// call, never log it, never keep it (TECH_SPEC §12). What reaches SQLite is an opaque key.
  func addXtream(name: String, server: String, username: String, password: String) async throws
    -> Source
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

/// One event from a live pairing session. `started` arrives once, then a `submission` per phone;
/// `failed` is terminal.
public enum PairingEvent: Sendable {
  /// The server is up — render this URL, port, and token.
  case started(PairingSession)
  /// A phone submitted details, ready to pre-fill the add-source flow.
  case submission(PairingSubmission)
  /// The server could not start. Terminal.
  case failed(ApiError)
}

/// The narrow core surface the **pairing** screen needs: bring the LAN server up, hear what a
/// phone sends, and take it down again (PRD §6.1, TECH_SPEC §12).
///
/// Modelled as a stream rather than a start/stop pair because the server's lifetime *is* the
/// security model — it exists only while its screen is visible. Tying it to a stream makes the
/// consuming task's lifetime enforce that: when the screen goes away its task ends, the stream
/// terminates, and the server comes down whether or not anyone remembered to say so.
/// `stopPairing` is still here because the core asks for the stop to be *prompt and awaited*, not
/// because it is the only thing standing between a closed screen and a listener on the LAN.
public protocol PairingAccess: Sendable {
  /// Starts the server advertising `host` — the TV's own LAN address, which **the shell must
  /// supply**. The core infers it from the route out of the host, which is right on a plain LAN
  /// and wrong behind a full-tunnel VPN, where the route is the tunnel and the LAN address sits on
  /// an interface the probe never sees (`core-pair`'s docs carry the measurements). `nil` asks for
  /// that inference and fails loudly rather than advertising an address no phone can dial.
  func pairing(host: String?) -> AsyncStream<PairingEvent>

  /// Stops the server now, without waiting for the stream's termination to get around to it.
  func stopPairing() async
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

/// One event from a running programme-guide refresh. The stream ends after either terminal event.
public enum EpgRefreshEvent: Sendable {
  case progress(EpgRefreshProgress)
  case complete(EpgRefreshOutcome)
  case failed(ApiError)
}

/// A bounded guide-window request. Keeping the paging and time bounds together prevents callers
/// from accidentally issuing an unbounded cross-boundary query.
public struct EpgWindowQuery: Sendable, Equatable {
  public let sourceId: Int64
  public let channelIdentity: Int64
  public let earliest: Date
  public let latest: Date
  public let offset: UInt32
  public let limit: UInt32

  public init(
    sourceId: Int64, channelIdentity: Int64, earliest: Date, latest: Date, offset: UInt32,
    limit: UInt32
  ) {
    self.sourceId = sourceId
    self.channelIdentity = channelIdentity
    self.earliest = earliest
    self.latest = latest
    self.offset = offset
    self.limit = limit
  }
}

/// The narrow guide surface used by channel details and source guide settings.
public protocol EpgAccess: Sendable {
  func nowNext(sourceId: Int64, channelIdentity: Int64, now: Date) async throws -> NowNext
  /// One bounded call per visible page. The core accepts at most 100 identities and preserves
  /// their order, including channels with no guide match.
  func nowNextBatch(sourceId: Int64, channelIdentities: [Int64], now: Date) async throws
    -> [ChannelNowNext]
  func epgWindow(_ query: EpgWindowQuery) async throws -> EpgPage
  func hasEpgFeed(sourceId: Int64) async throws -> Bool
  func setXmltvFeed(sourceId: Int64, url: String) async throws
  func clearXmltvFeed(sourceId: Int64) async throws
  func refreshEpg(sourceId: Int64, now: Date) -> AsyncStream<EpgRefreshEvent>
}

/// A request-header entry typed in the custom-channel editor.
public struct CustomHeaderInput: Sendable, Equatable, Identifiable {
  public let id: UUID
  public var name: String
  public var value: String

  public init(id: UUID = UUID(), name: String = "", value: String = "") {
    self.id = id
    self.name = name
    self.value = value
  }
}

/// The shell-owned editable representation of an opaque core custom-channel draft.
public struct CustomChannelInput: Sendable, Equatable {
  public var groupId: Int64?
  public var name: String
  public var logo: String
  public var streamAddress: String
  public var userAgent: String
  public var headers: [CustomHeaderInput]

  public init(
    groupId: Int64? = nil, name: String = "", logo: String = "", streamAddress: String = "",
    userAgent: String = "", headers: [CustomHeaderInput] = []
  ) {
    self.groupId = groupId
    self.name = name
    self.logo = logo
    self.streamAddress = streamAddress
    self.userAgent = userAgent
    self.headers = headers
  }
}

/// User-created channel and group management, including explicit portable sharing.
public protocol CustomChannelsAccess: Sendable {
  func customGroups() async throws -> [CustomGroup]
  func customChannels(groupId: Int64?) async throws -> [CustomChannelSummary]
  func createCustomChannel(_ input: CustomChannelInput) async throws -> Int64
  func updateCustomChannel(id: Int64, input: CustomChannelInput) async throws
  func deleteCustomChannel(id: Int64) async throws
  func moveCustomChannelBefore(id: Int64, anchorId: Int64) async throws
  func moveCustomChannelAfter(id: Int64, anchorId: Int64) async throws
  func createCustomGroup(name: String) async throws -> Int64
  func renameCustomGroup(id: Int64, name: String) async throws
  func deleteCustomGroup(id: Int64) async throws
  func moveCustomGroupBefore(id: Int64, anchorId: Int64) async throws
  func moveCustomGroupAfter(id: Int64, anchorId: Int64) async throws
  func exportCustomChannels() async throws -> String
  func importCustomChannels(_ contents: String, mode: CustomImportMode) async throws -> UInt64
}

/// The bounded favorite-lineup surface. Moves name one item and one adjacent anchor only.
public protocol FavoriteOrderingAccess: Sendable {
  func favoriteLineup(offset: UInt32, limit: UInt32) async throws -> [PlayableChannel]
  func moveFavoriteBefore(_ channel: PlayableChannel, anchor: PlayableChannel) async throws
  func moveFavoriteAfter(_ channel: PlayableChannel, anchor: PlayableChannel) async throws
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
