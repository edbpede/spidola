// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import CoreKit
import Foundation
import OSLog
import TVServices

/// Refreshes the extension's bounded projection from the core-owned favorites lineup.
@MainActor
enum TopShelfSnapshotWriter {
  static func refresh(from core: FavoriteOrderingAccess) async {
    do {
      let channels = try await core.favoriteLineup(
        offset: 0,
        limit: UInt32(TopShelfSnapshot.maximumItemCount)
      )
      let favorites = channels.map { channel in
        TopShelfSnapshot.Item(
          sourceId: channel.sourceId,
          identity: channel.identity,
          title: channel.name,
          group: channel.group,
          imageURL: safeImageURL(channel.logo)
        )
      }
      try TopShelfSnapshotStore.write(TopShelfSnapshot(favorites: favorites))
      TVTopShelfContentProvider.topShelfContentDidChange()
    } catch {
      logger.error(
        "Top Shelf snapshot refresh failed: \(String(describing: error), privacy: .public)")
    }
  }

  private static func safeImageURL(_ value: String?) -> URL? {
    guard let value, let url = URL(string: value) else { return nil }
    guard url.scheme == "https" || url.scheme == "http" else { return nil }
    return url
  }

  private static let logger = Logger(
    subsystem: "dev.spidola.tv",
    category: "spidola::top-shelf"
  )
}
