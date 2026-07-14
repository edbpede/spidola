// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import CoreKit
import OSLog
import Observation
import core_api

/// Backs the channel detail screen: the favorite/hidden flags for the toggles, and the play action
/// that records a recent (Phase 5 wires the actual engine). Toggle failures surface as a short
/// notice with the diagnostic detail kept in the log stream (PRD §6.3, §8.6), never swallowed.
@MainActor
@Observable
public final class ChannelDetailModel {
  public let channel: PlayableChannel
  public private(set) var isFavorite = false
  public private(set) var isHidden = false
  public private(set) var notice: String?

  private let access: any BrowseAccess
  private let logger = Logger(subsystem: "dev.spidola.tv", category: "spidola::browse")

  public init(channel: PlayableChannel, access: any BrowseAccess) {
    self.channel = channel
    self.access = access
  }

  public func load() async {
    do {
      isFavorite = try await access.isFavorite(
        sourceId: channel.sourceId, identity: channel.identity)
      isHidden = try await access.isHidden(sourceId: channel.sourceId, identity: channel.identity)
    } catch is CancellationError {
    } catch {
      logger.error("detail load failed: \(String(describing: error), privacy: .public)")
    }
  }

  public func toggleFavorite() async {
    let makeFavorite = !isFavorite
    do {
      try await access.setFavorite(
        sourceId: channel.sourceId, identity: channel.identity, favorite: makeFavorite)
      isFavorite = makeFavorite
    } catch {
      present(error)
    }
  }

  public func toggleHidden() async {
    let makeHidden = !isHidden
    do {
      try await access.setHidden(
        sourceId: channel.sourceId, identity: channel.identity, hidden: makeHidden)
      isHidden = makeHidden
    } catch {
      present(error)
    }
  }

  /// Records the channel to recently-watched. Playback itself lands with the engine contract in
  /// Phase 5; recording here means the recents row is exercised end-to-end now.
  public func play() async {
    do {
      try await access.recordRecent(channel)
      notice = "Saved to Recently watched. Full playback arrives in a later update."
    } catch {
      present(error)
    }
  }

  private func present(_ error: Error) {
    if error is CancellationError { return }
    let api = (error as? ApiError) ?? .Internal
    notice = ActionableError(api).message
    logger.error("detail action failed: \(String(describing: error), privacy: .public)")
  }
}
