// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import CoreKit
import DesignSystem
import SwiftUI

/// The browse vertical slice for the walking skeleton (M0): a D-pad-navigable list of the fixture
/// catalog's channels. Playback on select, and the source → type → category drill-down, land in
/// later phases; this view proves focus traversal and the core → shell rendering path.
public struct BrowseView: View {
  @State private var model: BrowseModel

  public init(catalog: any CatalogAccess) {
    _model = State(initialValue: BrowseModel(catalog: catalog))
  }

  public var body: some View {
    content
      .frame(maxWidth: .infinity, maxHeight: .infinity)
      .background(SpidolaPalette.studio)
      .task { await model.load() }
  }

  @ViewBuilder private var content: some View {
    switch model.state {
    case .loading:
      CenteredMessage(text: "Loading channels…")
    case .empty:
      CenteredMessage(text: "No sources yet — add one to start watching.")
    case .error(let message):
      CenteredMessage(text: message)
    case .ready(let channels):
      ChannelList(channels: channels)
    }
  }
}

private struct ChannelList: View {
  let channels: [ChannelItem]

  @FocusState private var focusedID: Int64?

  var body: some View {
    ScrollView {
      LazyVStack(spacing: SpidolaSpacing.s) {
        ForEach(channels) { item in
          ChannelRow(item: item, isFocused: focusedID == item.id)
            .focused($focusedID, equals: item.id)
        }
      }
      .padding(.horizontal, SpidolaSpacing.safeHorizontal)
      .padding(.vertical, SpidolaSpacing.safeVertical)
    }
  }
}

private struct ChannelRow: View {
  let item: ChannelItem
  let isFocused: Bool

  var body: some View {
    Button {
      // Selecting a channel starts playback in Phase 5.
    } label: {
      VStack(alignment: .leading, spacing: SpidolaSpacing.xs) {
        Text(item.name)
          .font(SpidolaType.body)
          .foregroundStyle(SpidolaPalette.broadcastWhite)
        if let group = item.group {
          Text(group)
            .font(SpidolaType.caption)
            .foregroundStyle(SpidolaPalette.staticGray)
        }
      }
      .frame(maxWidth: .infinity, alignment: .leading)
      .padding(SpidolaSpacing.m)
      .background(SpidolaPalette.set)
    }
    .buttonStyle(.plain)
    .spidolaFocusRing(isFocused: isFocused)
  }
}

private struct CenteredMessage: View {
  let text: String

  var body: some View {
    Text(text)
      .font(SpidolaType.title)
      .foregroundStyle(SpidolaPalette.broadcastWhite)
      .multilineTextAlignment(.center)
      .frame(maxWidth: .infinity, maxHeight: .infinity)
      .padding(SpidolaSpacing.xl)
  }
}
