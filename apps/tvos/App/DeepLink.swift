// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import Foundation

/// Stable platform links shared by the system-search activity and the Top Shelf extension.
enum DeepLink: Equatable {
  case home
  case search
  case sources
  case source(Int64)
  case channel(sourceId: Int64, identity: Int64)

  init?(_ url: URL) {
    guard url.scheme?.lowercased() == "spidola", let host = url.host?.lowercased() else {
      return nil
    }
    let parts = url.pathComponents.filter { $0 != "/" }
    switch (host, parts) {
    case ("home", []): self = .home
    case ("search", []): self = .search
    case ("sources", []): self = .sources
    case ("source", let parts) where parts.count == 1:
      guard let id = Int64(parts[0]) else { return nil }
      self = .source(id)
    case ("channel", let parts) where parts.count == 2:
      guard let sourceId = Int64(parts[0]), let identity = Int64(parts[1]) else { return nil }
      self = .channel(sourceId: sourceId, identity: identity)
    default:
      return nil
    }
  }
}
