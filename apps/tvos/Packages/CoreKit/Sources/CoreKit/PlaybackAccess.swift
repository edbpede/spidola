// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import core_api

/// The list a channel was played from, and therefore the ring that D-pad up/down zaps through
/// (PRD §8.4). It names the *query*, not its results: the core stays the single source of truth,
/// and a 50k-channel ring never crosses the FFI as a list.
///
/// Every arm maps onto a paged core query the shell already issues, so zapping costs one
/// three-row page rather than a new core surface.
public enum ZapContext: Hashable, Sendable {
  /// Played from the browse drill-down — zaps through that category.
  case group(sourceId: Int64, kind: MediaKind, group: String?)
  /// Played from the favourites row — zaps through favourites.
  case favorites
  /// Played from search — zaps through the result set.
  case search(query: String, sourceId: Int64?, kind: MediaKind?)
  /// Played from somewhere with no natural ring (a recent). Zap is unavailable and the strip
  /// shows no peek — an honest absence rather than a ring invented from one row.
  case single
}

/// The channel being played and its immediate neighbours — everything the channel strip's
/// adjacent-peek and the zap path need, in one page (PRD §8.5).
public struct ZapWindow: Sendable, Equatable {
  public let previous: PlayableChannel?
  public let current: PlayableChannel
  public let next: PlayableChannel?
  /// `current`'s position in the ring; zapping moves this by one.
  public let offset: UInt32
  /// The ring's length, so the strip can show position. `nil` when the ring's length is not
  /// knowable — the search query is scored and paged without a count, so a search ring reports an
  /// honest "unknown" rather than a total invented from the current page.
  public let total: UInt64?

  public init(
    previous: PlayableChannel?, current: PlayableChannel, next: PlayableChannel?,
    offset: UInt32, total: UInt64?
  ) {
    self.previous = previous
    self.current = current
    self.next = next
    self.offset = offset
    self.total = total
  }
}

/// One plaintext HTTP header returned only by the play-time resolver.
public struct ResolvedPlaybackHeader: Sendable, Equatable, CustomDebugStringConvertible {
  public let name: String
  public let value: String

  public init(name: String, value: String) {
    self.name = name
    self.value = value
  }

  public var debugDescription: String {
    "ResolvedPlaybackHeader(name: \(name), value: [REDACTED])"
  }
}

/// Ephemeral engine input after the core has opened every authenticated catalog envelope.
public struct ResolvedPlaybackStream: Sendable, Equatable, CustomDebugStringConvertible {
  public let locator: String
  public let userAgent: String?
  public let headers: [ResolvedPlaybackHeader]

  public init(locator: String, userAgent: String?, headers: [ResolvedPlaybackHeader]) {
    self.locator = locator
    self.userAgent = userAgent
    self.headers = headers
  }

  public var debugDescription: String {
    let redactedUserAgent = userAgent == nil ? "nil" : "[REDACTED]"
    return
      "ResolvedPlaybackStream(locator: [REDACTED], userAgent: \(redactedUserAgent), headers: \(headers))"
  }
}

/// The narrow core surface the **playback** slice needs: the zap ring, the persisted engine
/// overrides the selection policy reads, the engine-neutral playback settings, and the play-time
/// recents record.
///
/// Engine overrides are **opaque strings** here, never `EngineID`: CoreKit must not depend on
/// PlayerContract (engine identity is the player layer's concept, and the core already persists it
/// as an opaque key — TECH_SPEC §8). The playback slice, which depends on both, does the mapping.
public protocol PlaybackAccess: Sendable {
  /// Resolves the channel at `offset` in `context` plus its neighbours. Returns `nil` when the
  /// context has no ring (`.single`) or the ring no longer has a row there — a catalog refresh can
  /// move offsets under a playing channel, and inventing a neighbour would zap the viewer somewhere
  /// they did not ask for.
  func zapWindow(context: ZapContext, offset: UInt32) async throws -> ZapWindow?

  /// The "remember for this channel" engine choice, if the viewer set one.
  func channelEngine(sourceId: Int64, identity: Int64) async throws -> String?
  /// Sets or (with `nil`) clears the per-channel engine choice.
  func setChannelEngine(sourceId: Int64, identity: Int64, engine: String?) async throws
  /// The per-source engine choice, if set.
  func sourceEngine(sourceId: Int64) async throws -> String?

  /// The engine-neutral buffering profile, as its raw key; `nil` means the app default.
  func bufferingProfile() async throws -> String?
  func setBufferingProfile(_ profile: String) async throws

  /// The playable URL and request overrides for a channel. Call this immediately before handing
  /// the result to an engine, and never store it.
  ///
  /// Not a formality: an Xtream catalog stores a **credential-free** locator so the account's
  /// password never reaches SQLite (TECH_SPEC §12), which means the locator on a `PlayableChannel`
  /// is not playable on its own — this is what puts the credential back. Kind-agnostic, so the zap
  /// path never branches on where a channel came from. M3U locators and override values remain
  /// authenticated envelopes until this call.
  func resolvePlayback(_ channel: PlayableChannel) async throws -> ResolvedPlaybackStream

  func recordRecent(_ channel: PlayableChannel) async throws
}

/// Translates the buffering profile between the core's boundary enum and the raw value
/// `PlaybackAccess` speaks.
///
/// There is no key-naming type here any more, and that is the point: the core owns the settings
/// vocabulary now, so the engine overrides are reached through `engineForChannel` /
/// `engineForSource` / `setDefaultEngine` rather than through key strings this module used to
/// invent. Where those choices *live* — the settings table, keyed on the stable identity hash and
/// never on `channels.preferred_engine`, because a refresh replaces every channel row wholesale
/// (TECH_SPEC §4.4) — is now documented on the core's own `channel_engine` key, which is the thing
/// that decides it.
///
/// The core's `BufferingProfile` mirrors `PlayerContract`'s case for case and spelling for
/// spelling — the core adopted the vocabulary Phase 5 had already settled rather than inventing a
/// second, lossy one — so this adapter is a straight relabelling and not a lossy translation. It
/// exists at all only because **CoreKit must not import `PlayerContract`**: engine identity and
/// engine tuning are the player layer's concepts (TECH_SPEC §8), and a dependency from the binding
/// wrapper onto the player contract would invert that. So the two enums cannot simply be one type,
/// and the raw value is the seam where they meet.
///
/// `BufferingBridgeTests` (in the settings slice's suite — the only target that depends on both
/// modules, and so the only place the two vocabularies can be checked against each other) pins it:
/// a setting the viewer changes has to reach the player, and these two drifting apart is exactly
/// where that would quietly stop being true without anything failing to compile.
extension BufferingProfile {
  /// The raw value the playback slice reads back into its own profile.
  var playbackKey: String {
    switch self {
    case .low: "low"
    case .balanced: "balanced"
    case .generous: "generous"
    @unknown default: "balanced"
    }
  }

  /// Reads a raw playback value back into the core's enum, falling back to the shared default
  /// rather than throwing: the value comes from persisted settings, so an unrecognized one means a
  /// newer app wrote it, not that this caller made a mistake.
  init(playbackKey: String) {
    switch playbackKey {
    case "low": self = .low
    case "generous": self = .generous
    default: self = .balanced
    }
  }
}
