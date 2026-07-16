// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import CoreKit
import Foundation
import OSLog
import Observation
import core_api

/// Schedule state for a browse row. Pending and unavailable deliberately render at the same fixed
/// height; the distinction lets tests and accessibility tell an unanswered lookup from an empty
/// guide without putting a spinner in a focusable list.
public enum ChannelSchedule: Sendable, Equatable {
  case pending
  case unavailable
  case ready(NowNext)
}

/// One channel row plus its favorite flag, so the list marks favorites without an `isFavorite`
/// call per row. `id` is the stable identity, so it survives a refresh as the list/focus key.
public struct ChannelRow: Identifiable, Sendable, Equatable {
  public let channel: Channel
  public var isFavorite: Bool
  public var schedule: ChannelSchedule = .pending
  public var id: Int64 { channel.identity }
}

/// The channel level of the browse drill-down: the visible channels in one group, paged by
/// contract and appended as the user scrolls (virtualized), with the per-channel favorite and hide
/// actions the context menu drives. Hidden channels are excluded by the core, so hiding one simply
/// drops it from the list.
@MainActor
@Observable
public final class ChannelsModel {
  public private(set) var state: LoadState<[ChannelRow]> = .loading

  private let sourceId: Int64
  private let kind: MediaKind
  private let group: String?
  private let access: any BrowseAccess
  private let epg: any EpgAccess

  private var favorites: Set<Int64> = []
  private var rows: [ChannelRow] = []
  private var total: UInt64 = 0
  private var isPaging = false
  /// Matches the core's hard upper bound for `nowNextBatch`, so each channel page needs exactly
  /// one bounded guide query.
  private static let pageLimit: UInt32 = 100
  private static let prefetchMargin = 20

  public init(
    sourceId: Int64, kind: MediaKind, group: String?, access: any BrowseAccess, epg: any EpgAccess
  ) {
    self.sourceId = sourceId
    self.kind = kind
    self.group = group
    self.access = access
    self.epg = epg
  }

  /// The ring a channel opened from this list zaps through: this list's own query (PRD §8.4).
  public var zapContext: ZapContext { .group(sourceId: sourceId, kind: kind, group: group) }

  /// `row`'s absolute position in the ring, or `nil` once it has left the list.
  ///
  /// Pages are appended in order from offset 0 and a hidden row leaves both this list and the
  /// core's, so a row's index here is its offset in the core's — the value the zap ring is keyed
  /// on. Callers resolve this on selection, never per render.
  public func offset(of row: ChannelRow) -> UInt32? {
    rows.firstIndex(where: { $0.id == row.id }).map(UInt32.init)
  }

  public func load() async {
    state = .loading
    favorites = []
    rows = []
    total = 0
    do {
      favorites = Set(try await access.favoriteIdentities(sourceId: sourceId))
      let page = try await access.channelsInGroup(
        sourceId: sourceId, kind: kind, group: group, offset: 0, limit: Self.pageLimit)
      total = page.total
      append(page.channels)
      state = rows.isEmpty ? .empty : .ready(rows)
      await loadSchedules(for: page.channels)
    } catch {
      if let failed = LoadState<[ChannelRow]>.failure(from: error) { state = failed }
    }
  }

  /// Loads the next page when the given row nears the loaded tail (virtualized paging). Paging
  /// failures keep what is already shown rather than replacing the list with an error.
  public func loadMoreIfNeeded(after row: ChannelRow) async {
    guard case .ready = state, !isPaging, UInt64(rows.count) < total,
      let index = rows.firstIndex(of: row), index >= rows.count - Self.prefetchMargin
    else { return }
    isPaging = true
    defer { isPaging = false }
    do {
      let offset = UInt32(rows.count)
      let page = try await access.channelsInGroup(
        sourceId: sourceId, kind: kind, group: group, offset: offset, limit: Self.pageLimit)
      append(page.channels)
      state = .ready(rows)
      await loadSchedules(for: page.channels)
    } catch is CancellationError {
    } catch {
      // Keep the rows already loaded; the next scroll retries.
    }
  }

  /// Loads a whole page's schedule in one core call. Rows are already visible with fixed-height
  /// placeholders while this suspends; applying the result mutates values behind stable IDs, so
  /// focus does not move when the schedule crossfades in.
  private func loadSchedules(for channels: [Channel], at now: Date = .now) async {
    guard !channels.isEmpty else { return }
    let identities = channels.map(\.identity)
    precondition(identities.count <= Int(Self.pageLimit))
    do {
      let values = try await epg.nowNextBatch(
        sourceId: sourceId, channelIdentities: identities, now: now)
      guard !Task.isCancelled else { return }
      let schedules = Dictionary(
        uniqueKeysWithValues: values.map { value in
          let programmes = value.programmes
          let schedule: ChannelSchedule =
            programmes.current == nil && programmes.next == nil
            ? .unavailable : .ready(programmes)
          return (value.channelIdentity, schedule)
        })
      applySchedules(schedules, to: identities)
    } catch is CancellationError {
    } catch {
      guard !Task.isCancelled else { return }
      Self.logger.error(
        "guide batch failed: \(String(describing: error), privacy: .public)")
      applySchedules(
        Dictionary(uniqueKeysWithValues: identities.map { ($0, ChannelSchedule.unavailable) }),
        to: identities)
    }
  }

  public func toggleFavorite(_ row: ChannelRow) async {
    let makeFavorite = !favorites.contains(row.id)
    setFavorite(row.id, makeFavorite)  // optimistic
    do {
      try await access.setFavorite(
        sourceId: sourceId, identity: row.channel.identity, favorite: makeFavorite)
    } catch {
      setFavorite(row.id, !makeFavorite)  // revert on failure
    }
  }

  public func hide(_ row: ChannelRow) async {
    do {
      try await access.setHidden(sourceId: sourceId, identity: row.channel.identity, hidden: true)
      rows.removeAll { $0.id == row.id }
      state = rows.isEmpty ? .empty : .ready(rows)
    } catch {
      // Leave the row in place; the user can try again.
    }
  }

  private func append(_ channels: [Channel]) {
    for channel in channels {
      rows.append(
        ChannelRow(channel: channel, isFavorite: favorites.contains(channel.identity)))
    }
  }

  private func applySchedules(_ schedules: [Int64: ChannelSchedule], to identities: [Int64]) {
    let pageIdentities = Set(identities)
    for index in rows.indices where pageIdentities.contains(rows[index].id) {
      rows[index].schedule = schedules[rows[index].id] ?? .unavailable
    }
    if case .ready = state { state = .ready(rows) }
  }

  private func setFavorite(_ id: Int64, _ isFavorite: Bool) {
    if isFavorite { favorites.insert(id) } else { favorites.remove(id) }
    if let index = rows.firstIndex(where: { $0.id == id }) { rows[index].isFavorite = isFavorite }
    if case .ready = state { state = .ready(rows) }
  }

  private static let logger = Logger(
    subsystem: "dev.spidola.tv",
    category: "spidola::browse"
  )
}
