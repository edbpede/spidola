// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import CoreKit
import Observation
import core_api

/// The type → category level of the browse drill-down for one source. Loads the media kinds
/// present, then the groups for the selected kind. For an M3U source there is a single kind
/// (`Live`), so the kind selector never appears (PRD §8.3 drill-down).
public struct SourceBrowseContent: Sendable {
  public let kinds: [MediaKind]
  public let kind: MediaKind
  public let groups: [BrowseGroup]
}

@MainActor
@Observable
public final class SourceBrowseModel {
  public private(set) var state: LoadState<SourceBrowseContent> = .loading

  private let sourceId: Int64
  private let access: any BrowseAccess
  // Groups are bounded (distinct playlist categories — dozens to a few hundred) and virtualized in
  // the list; one generous page is loaded and the display lazily renders it.
  private static let groupLimit: UInt32 = 1000

  public init(sourceId: Int64, access: any BrowseAccess) {
    self.sourceId = sourceId
    self.access = access
  }

  public func load() async {
    state = .loading
    do {
      let kinds = try await access.kinds(sourceId: sourceId)
      guard let first = kinds.first else {
        state = .empty
        return
      }
      try await loadGroups(kinds: kinds, kind: first)
    } catch {
      if let failed = LoadState<SourceBrowseContent>.failure(from: error) { state = failed }
    }
  }

  public func select(kind: MediaKind) async {
    guard case .ready(let content) = state, content.kind != kind else { return }
    do {
      try await loadGroups(kinds: content.kinds, kind: kind)
    } catch {
      if let failed = LoadState<SourceBrowseContent>.failure(from: error) { state = failed }
    }
  }

  private func loadGroups(kinds: [MediaKind], kind: MediaKind) async throws {
    let groups = try await access.groups(
      sourceId: sourceId, kind: kind, offset: 0, limit: Self.groupLimit
    ).groups
    state =
      groups.isEmpty
      ? .empty
      : .ready(SourceBrowseContent(kinds: kinds, kind: kind, groups: groups))
  }
}
