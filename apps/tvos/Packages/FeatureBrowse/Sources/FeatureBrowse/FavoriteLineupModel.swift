// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import CoreKit
import Observation

@MainActor
@Observable
public final class FavoriteLineupModel {
  public private(set) var state: LoadState<[PlayableChannel]> = .loading

  private let access: any FavoriteOrderingAccess
  private static let pageLimit: UInt32 = 200

  public init(access: any FavoriteOrderingAccess) {
    self.access = access
  }

  public func load() async {
    state = .loading
    do {
      var channels: [PlayableChannel] = []
      while true {
        let page = try await access.favoriteLineup(
          offset: UInt32(channels.count), limit: Self.pageLimit)
        channels.append(contentsOf: page)
        if page.count < Int(Self.pageLimit) { break }
      }
      state = channels.isEmpty ? .empty : .ready(channels)
    } catch {
      if let failed = LoadState<[PlayableChannel]>.failure(from: error) { state = failed }
    }
  }

  public func moveUp(_ channel: PlayableChannel) async {
    guard case .ready(let channels) = state,
      let index = channels.firstIndex(of: channel), index > channels.startIndex
    else { return }
    await move {
      try await access.moveFavoriteBefore(channel, anchor: channels[index - 1])
    }
  }

  public func moveDown(_ channel: PlayableChannel) async {
    guard case .ready(let channels) = state,
      let index = channels.firstIndex(of: channel),
      index < channels.index(before: channels.endIndex)
    else { return }
    await move {
      try await access.moveFavoriteAfter(channel, anchor: channels[index + 1])
    }
  }

  private func move(_ action: () async throws -> Void) async {
    do {
      try await action()
      await load()
    } catch {
      if let failed = LoadState<[PlayableChannel]>.failure(from: error) { state = failed }
    }
  }
}
