// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import CoreKit
import DesignSystem
import SwiftUI
import core_api

/// The channel level of the drill-down: the visible channels in a group, D-pad-focusable and
/// virtualized, each with a context menu (open, favorite, hide, and the Phase-5 engine override).
/// Selecting a channel opens its detail; the star accessory marks favorites.
public struct ChannelsView: View {
  @State private var model: ChannelsModel
  private let title: String
  private let navigator: BrowseNavigator

  @FocusState private var focused: Int64?

  public init(
    sourceId: Int64, kind: MediaKind, group: String?, title: String,
    access: any BrowseAccess & EpgAccess, navigator: BrowseNavigator
  ) {
    _model = State(
      initialValue: ChannelsModel(
        sourceId: sourceId, kind: kind, group: group, access: access, epg: access))
    self.title = title
    self.navigator = navigator
  }

  public var body: some View {
    content
      .frame(maxWidth: .infinity, maxHeight: .infinity)
      .background(SpidolaPalette.studio)
      .navigationTitle(title)
      .task { await model.load() }
  }

  @ViewBuilder private var content: some View {
    switch model.state {
    case .loading:
      ProgressView(String(localized: "Loading channels…", bundle: .module))
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    case .empty:
      CenteredNotice(text: String(localized: "No channels here.", bundle: .module))
    case .failed(let error):
      actionableError(error, retry: { Task { await model.load() } }, goBack: {})
    case .ready(let rows):
      list(rows)
    }
  }

  private func list(_ rows: [ChannelRow]) -> some View {
    ScrollView {
      LazyVStack(spacing: SpidolaSpacing.s) {
        ForEach(rows) { row in
          channelRow(row)
        }
      }
      .padding(.horizontal, SpidolaSpacing.safeHorizontal)
      .padding(.vertical, SpidolaSpacing.safeVertical)
    }
  }

  private func channelRow(_ row: ChannelRow) -> some View {
    ChannelScheduleRow(row: row, isFocused: focused == row.id) {
      open(row)
    }
    .focused($focused, equals: row.id)
    // The star is the only thing marking a favorite, and a glyph has no voice. Only favorites get
    // a value: in a list where most channels are not one, saying so on every row would bury the
    // handful that are under the ones that aren't.
    .accessibilityLabel(Self.label(for: row.channel))
    .accessibilityValue(Self.accessibilityValue(for: row))
    .accessibilityIdentifier("channel-\(row.channel.name)")
    .contextMenu {
      Button(String(localized: "Open", bundle: .module)) { open(row) }
      Button(
        row.isFavorite
          ? String(localized: "Remove favorite", bundle: .module)
          : String(localized: "Add favorite", bundle: .module)
      ) {
        Task { await model.toggleFavorite(row) }
      }
      Button(String(localized: "Hide channel", bundle: .module), role: .destructive) {
        Task { await model.hide(row) }
      }
      // Per-channel engine override lands with the player contract in Phase 5.
      Button(String(localized: "Player: Default", bundle: .module)) {}
        .disabled(true)
    }
    .onAppear { Task { await model.loadMoreIfNeeded(after: row) } }
  }

  private func open(_ row: ChannelRow) {
    guard let offset = model.offset(of: row) else { return }
    navigator.openChannel(PlayableChannel(row.channel), model.zapContext, offset)
  }

  /// Name and group as one phrase. Naming a row at all replaces everything it would otherwise say
  /// for itself, and the group is half of how a viewer tells two channels with near-identical
  /// names apart — dropping it to make room for the favorite value would trade one loss for
  /// another.
  private static func label(for channel: Channel) -> String {
    channel.groupTitle.map { String(localized: "\(channel.name), \($0)", bundle: .module) }
      ?? channel.name
  }

  private static func accessibilityValue(for row: ChannelRow) -> String {
    var parts: [String] = []
    if row.isFavorite { parts.append(String(localized: "Favorite", bundle: .module)) }
    if case .ready(let nowNext) = row.schedule {
      if let current = nowNext.current {
        parts.append(String(localized: "Now: \(current.title)", bundle: .module))
      }
      if let next = nowNext.next {
        parts.append(String(localized: "Next: \(next.title)", bundle: .module))
      }
    } else {
      parts.append(String(localized: "Schedule unavailable", bundle: .module))
    }
    return parts.joined(separator: ", ")
  }
}
