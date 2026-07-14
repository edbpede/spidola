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
    access: any BrowseAccess, navigator: BrowseNavigator
  ) {
    _model = State(
      initialValue: ChannelsModel(sourceId: sourceId, kind: kind, group: group, access: access))
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
      ProgressView("Loading channels…")
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    case .empty:
      CenteredNotice(text: "No channels here.")
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
    SpidolaRow(
      title: row.channel.name,
      subtitle: row.channel.groupTitle,
      accessory: row.isFavorite ? .symbol("star.fill") : .none,
      isFocused: focused == row.id
    ) {
      navigator.openChannel(PlayableChannel(row.channel))
    }
    .focused($focused, equals: row.id)
    .accessibilityIdentifier("channel-\(row.channel.name)")
    .contextMenu {
      Button("Open") { navigator.openChannel(PlayableChannel(row.channel)) }
      Button(row.isFavorite ? "Remove favorite" : "Add favorite") {
        Task { await model.toggleFavorite(row) }
      }
      Button("Hide channel", role: .destructive) { Task { await model.hide(row) } }
      // Per-channel engine override lands with the player contract in Phase 5.
      Button("Player: Default") {}
        .disabled(true)
    }
    .onAppear { Task { await model.loadMoreIfNeeded(after: row) } }
  }
}
