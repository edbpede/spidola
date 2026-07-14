// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import CoreKit
import Observation
import core_api

/// Search results plus whether the trigram fuzzy fallback produced them (so the UI can say so).
public struct SearchResults: Sendable {
  public let channels: [Channel]
  public let fuzzy: Bool
}

/// The search screen's phase. `idle` is the empty-query resting state; the rest mirror a load.
public enum SearchState: Sendable {
  case idle
  case loading
  case empty
  case results(SearchResults)
  case failed(ActionableError)
}

/// Drives global search with per-keystroke results against the core's sub-50 ms budget (PRD §9),
/// plus the source and media-kind filters. Keystrokes are debounced and the in-flight query is
/// cancelled when a newer one arrives, so typing never queues a backlog. Depends on the narrow
/// `SearchAccess`.
@MainActor
@Observable
public final class SearchModel {
  public var query = ""
  public var sourceFilter: Int64?
  public var kindFilter: MediaKind?
  public private(set) var sources: [Source] = []
  public private(set) var state: SearchState = .idle

  private let access: any SearchAccess
  private var searchTask: Task<Void, Never>?
  private static let debounce = Duration.milliseconds(120)
  private static let pageLimit: UInt32 = 100

  public init(access: any SearchAccess) {
    self.access = access
  }

  public func loadSources() async {
    sources = (try? await access.sources()) ?? []
  }

  /// Schedules a debounced search for the current query and filters, cancelling any in-flight one.
  public func scheduleSearch() {
    searchTask?.cancel()
    let query = self.query.trimmingCharacters(in: .whitespacesAndNewlines)
    let sourceId = sourceFilter
    let kind = kindFilter
    guard !query.isEmpty else {
      state = .idle
      return
    }
    state = .loading
    searchTask = Task { [weak self] in
      do {
        try await Task.sleep(for: Self.debounce)
      } catch {
        return  // superseded by a newer keystroke
      }
      await self?.run(query: query, sourceId: sourceId, kind: kind)
    }
  }

  /// Awaits the in-flight (debounced) search task. Test-only seam; production code observes
  /// `state` instead.
  func waitForSearch() async {
    await searchTask?.value
  }

  private func run(query: String, sourceId: Int64?, kind: MediaKind?) async {
    do {
      let page = try await access.search(
        query: query, sourceId: sourceId, kind: kind, offset: 0, limit: Self.pageLimit)
      if Task.isCancelled { return }
      state =
        page.channels.isEmpty
        ? .empty
        : .results(SearchResults(channels: page.channels, fuzzy: page.fuzzy))
    } catch is CancellationError {
    } catch let error as ApiError {
      state = .failed(ActionableError(error))
    } catch {
      state = .failed(ActionableError(.Internal))
    }
  }
}
