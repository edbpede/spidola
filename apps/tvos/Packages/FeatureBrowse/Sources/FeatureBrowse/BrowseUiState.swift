// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

/// The browse screen's state. A closed set, matched exhaustively by the view.
public enum BrowseUiState: Sendable {
  case loading
  case empty
  case ready([ChannelItem])
  case error(String)
}

/// A single channel row's display data. `id` is the stable per-source identity (not the rowid),
/// so it doubles as the SwiftUI list identity and the focus key across a catalog refresh.
public struct ChannelItem: Identifiable, Hashable, Sendable {
  public let id: Int64
  public let name: String
  public let group: String?

  public init(id: Int64, name: String, group: String?) {
    self.id = id
    self.name = name
    self.group = group
  }
}
