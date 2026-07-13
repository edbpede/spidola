// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import CoreKit
import OSLog
import Observation
import core_api

/// Loads the first source's first page of channels for the walking skeleton (M0) and exposes it
/// as observable UI state on the main actor. It depends on the narrow `CatalogAccess` protocol,
/// not the concrete core, so it is testable against a fake. Drill-down is Phase 4.
@MainActor
@Observable
public final class BrowseModel {
  public private(set) var state: BrowseUiState = .loading

  private let catalog: any CatalogAccess
  private let logger = Logger(subsystem: "dev.spidola.tv", category: "spidola::browse")

  private static let pageLimit: UInt32 = 200

  public init(catalog: any CatalogAccess) {
    self.catalog = catalog
  }

  public func load() async {
    state = .loading
    do {
      guard let source = try await catalog.sources().first else {
        state = .empty
        return
      }
      let page = try await catalog.page(sourceId: source.id, offset: 0, limit: Self.pageLimit)
      state = page.channels.isEmpty ? .empty : .ready(page.channels.map(Self.item(from:)))
    } catch is CancellationError {
      // Cancellation propagates end-to-end; never swallowed.
    } catch {
      // Phase 4 maps each ApiError variant to a plain-language class + actions (PRD §6.3); the
      // diagnostic chain stays in the log stream, not on screen (PRD §8.6).
      logger.error("channel load failed: \(String(describing: error), privacy: .public)")
      state = .error("Couldn't load channels — try again.")
    }
  }

  private static func item(from channel: Channel) -> ChannelItem {
    ChannelItem(id: channel.identity, name: channel.name, group: channel.groupTitle)
  }
}
