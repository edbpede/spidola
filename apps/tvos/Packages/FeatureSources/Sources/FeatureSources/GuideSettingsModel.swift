// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import CoreKit
import Foundation
import Observation

public struct GuideSettingsSnapshot: Sendable {
  public let hasFeed: Bool
}

public enum GuideRefreshStatus: Sendable, Equatable {
  case idle
  case running(programmesSeen: UInt64)
  case complete(inserted: UInt64)
}

@MainActor
@Observable
public final class GuideSettingsModel {
  public var feedUrl = ""
  public private(set) var state: LoadState<GuideSettingsSnapshot> = .loading
  public private(set) var refreshStatus: GuideRefreshStatus = .idle

  private let sourceId: Int64
  private let access: any EpgAccess

  public init(sourceId: Int64, access: any EpgAccess) {
    self.sourceId = sourceId
    self.access = access
  }

  public func load() async {
    do {
      state = .ready(
        GuideSettingsSnapshot(hasFeed: try await access.hasEpgFeed(sourceId: sourceId)))
    } catch {
      if let failed = LoadState<GuideSettingsSnapshot>.failure(from: error) { state = failed }
    }
  }

  public func saveFeed() async {
    do {
      try await access.setXmltvFeed(sourceId: sourceId, url: feedUrl)
      feedUrl = ""
      await load()
    } catch {
      if let failed = LoadState<GuideSettingsSnapshot>.failure(from: error) { state = failed }
    }
  }

  public func clearFeed() async {
    do {
      try await access.clearXmltvFeed(sourceId: sourceId)
      feedUrl = ""
      refreshStatus = .idle
      await load()
    } catch {
      if let failed = LoadState<GuideSettingsSnapshot>.failure(from: error) { state = failed }
    }
  }

  public func refresh(now: Date = .now) async {
    refreshStatus = .running(programmesSeen: 0)
    for await event in access.refreshEpg(sourceId: sourceId, now: now) {
      switch event {
      case .progress(let progress):
        refreshStatus = .running(programmesSeen: progress.programmesSeen)
      case .complete(let outcome):
        refreshStatus = .complete(inserted: outcome.inserted)
      case .failed(let error):
        state = .failed(ActionableError(error))
      }
    }
  }
}
