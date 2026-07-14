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

  func recordRecent(_ channel: PlayableChannel) async throws
}

/// The settings keys the playback slice owns.
///
/// Engine overrides live in the settings table rather than `channels.preferred_engine` on purpose:
/// the channels table is replaced wholesale by the staging-and-swap refresh (TECH_SPEC §4.4), so a
/// choice stored there would silently vanish on the next refresh. Favourites and hidden survive
/// refresh by living in their own tables keyed on the stable identity hash; a remembered engine is
/// the same kind of durable per-channel fact and is keyed the same way.
enum PlaybackSettingKey {
  static func channelEngine(sourceId: Int64, identity: Int64) -> String {
    "player.engine.channel.\(sourceId).\(identity)"
  }

  static func sourceEngine(sourceId: Int64) -> String {
    "player.engine.source.\(sourceId)"
  }

  static let bufferingProfile = "player.buffering"
}
