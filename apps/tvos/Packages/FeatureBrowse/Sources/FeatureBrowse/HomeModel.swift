// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import CoreKit
import OSLog
import Observation
import core_api

/// The home screen's content: the enabled sources to browse, plus the favorites and recents rows
/// (PRD §8.3). Recents are empty when the off-switch is set (PRD §6.5), so the view simply omits
/// the row; `recentsEnabled` still drives the off-switch control.
public struct HomeContent: Sendable {
  public let sources: [Source]
  public let favorites: [PlayableChannel]
  public let recents: [PlayableChannel]
  public let recentsEnabled: Bool
}

/// Loads the home screen — the source list first, then the favorites and recents rows — and exposes
/// it as observable main-actor state. Depends on the narrow `HomeAccess`, so it is unit-tested
/// against a fake (TECH_SPEC §10).
@MainActor
@Observable
public final class HomeModel {
  public private(set) var state: LoadState<HomeContent> = .loading

  private let access: any HomeAccess
  private let logger = Logger(subsystem: "dev.spidola.tv", category: "spidola::browse")
  private static let rowLimit: UInt32 = 60

  public init(access: any HomeAccess) {
    self.access = access
  }

  public func load() async {
    state = .loading
    do {
      let sources = try await access.sources()
      guard sources.contains(where: { $0.common.enabled }) else {
        state = .empty
        return
      }
      let favorites = try await access.favoriteChannels(offset: 0, limit: Self.rowLimit)
        .channels.map(PlayableChannel.init)
      let recentsEnabled = try await access.recentsEnabled()
      var recents: [PlayableChannel] = []
      if recentsEnabled {
        recents = try await access.recents(limit: Self.rowLimit).map(PlayableChannel.init)
      }
      state = .ready(
        HomeContent(
          sources: sources, favorites: favorites, recents: recents,
          recentsEnabled: recentsEnabled))
    } catch {
      if let failed = LoadState<HomeContent>.failure(from: error) { state = failed }
    }
  }

  /// Toggles the recently-watched off-switch (PRD §6.5) and reloads so the row appears/disappears.
  public func setRecentsEnabled(_ enabled: Bool) async {
    await run { try await self.access.setRecentsEnabled(enabled) }
  }

  /// Purges the recently-watched list (PRD §6.5) and reloads.
  public func clearRecents() async {
    await run { try await self.access.clearRecents() }
  }

  /// Runs a user action, reloading on success so the UI reflects the core, and logging any failure
  /// (the diagnostic stays in the log stream, PRD §8.6) rather than swallowing it.
  private func run(_ action: () async throws -> Void) async {
    do {
      try await action()
      await load()
    } catch is CancellationError {
    } catch {
      logger.error("recents action failed: \(String(describing: error), privacy: .public)")
    }
  }
}
