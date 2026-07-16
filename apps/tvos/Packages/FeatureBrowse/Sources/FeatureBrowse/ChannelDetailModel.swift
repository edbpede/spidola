// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import CoreKit
import Foundation
import OSLog
import Observation
import core_api

/// Backs the channel detail screen: the favorite/hidden flags behind the toggles. Play is not here
/// — it is a navigation intent the shell owns, and the recent is recorded by the playback slice once
/// the stream actually starts. Toggle failures surface as a short notice with the diagnostic detail
/// kept in the log stream (PRD §6.3, §8.6), never swallowed.
@MainActor
@Observable
public final class ChannelDetailModel {
  public let channel: PlayableChannel
  public private(set) var isFavorite = false
  public private(set) var isHidden = false
  public private(set) var nowNext = NowNext(current: nil, next: nil)
  public private(set) var upcoming: [EpgProgramme] = []
  public private(set) var scheduleUnavailable = false
  public private(set) var notice: String?

  private let access: any BrowseAccess
  private let epg: any EpgAccess
  private let logger = Logger(subsystem: "dev.spidola.tv", category: "spidola::browse")

  public init(channel: PlayableChannel, access: any BrowseAccess, epg: any EpgAccess) {
    self.channel = channel
    self.access = access
    self.epg = epg
  }

  public func load(at now: Date = .now) async {
    do {
      isFavorite = try await access.isFavorite(
        sourceId: channel.sourceId, identity: channel.identity)
      isHidden = try await access.isHidden(sourceId: channel.sourceId, identity: channel.identity)
    } catch is CancellationError {
    } catch {
      logger.error("detail load failed: \(String(describing: error), privacy: .public)")
    }

    do {
      nowNext = try await epg.nowNext(
        sourceId: channel.sourceId, channelIdentity: channel.identity, now: now)
      upcoming = try await epg.epgWindow(
        EpgWindowQuery(
          sourceId: channel.sourceId, channelIdentity: channel.identity, earliest: now,
          latest: now.addingTimeInterval(6 * 60 * 60), offset: 0, limit: 8)
      ).programmes
      scheduleUnavailable = nowNext.current == nil && nowNext.next == nil && upcoming.isEmpty
    } catch is CancellationError {
    } catch {
      scheduleUnavailable = true
      logger.error("guide load failed: \(String(describing: error), privacy: .public)")
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

  private func present(_ error: Error) {
    if error is CancellationError { return }
    let api = (error as? ApiError) ?? .Internal
    notice = ActionableError(api).message
    logger.error("detail action failed: \(String(describing: error), privacy: .public)")
  }
}
