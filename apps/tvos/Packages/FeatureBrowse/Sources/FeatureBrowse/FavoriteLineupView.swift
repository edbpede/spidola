// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import CoreKit
import DesignSystem
import Foundation
import SwiftUI

/// A numbered, remote-friendly favorite lineup. Each mutation names one adjacent anchor only.
public struct FavoriteLineupView: View {
  @State private var model: FavoriteLineupModel
  @FocusState private var focused: Focus?

  public init(access: any FavoriteOrderingAccess) {
    _model = State(initialValue: FavoriteLineupModel(access: access))
  }

  public var body: some View {
    content
      .frame(maxWidth: .infinity, maxHeight: .infinity)
      .background(SpidolaPalette.studio)
      .navigationTitle(String(localized: "Favorite lineup", bundle: .module))
      .task { await model.load() }
  }

  @ViewBuilder private var content: some View {
    switch model.state {
    case .loading:
      ProgressView(String(localized: "Loading favorites…", bundle: .module))
    case .empty:
      ContentUnavailableView(
        String(localized: "No favorites yet", bundle: .module), systemImage: "star",
        description: Text(String(localized: "Favorite a channel to add it here.", bundle: .module)))
    case .failed(let error):
      actionableError(
        error, retry: { Task { await model.load() } },
        goBack: { Task { await model.load() } })
    case .ready(let channels):
      lineup(channels)
    }
  }

  private func lineup(_ channels: [PlayableChannel]) -> some View {
    ScrollView {
      LazyVStack(spacing: SpidolaSpacing.s) {
        ForEach(Array(channels.enumerated()), id: \.element.id) { index, channel in
          HStack(spacing: SpidolaSpacing.l) {
            Text(String(format: "%02d", index + 1))
              .font(SpidolaType.title)
              .foregroundStyle(SpidolaPalette.staticGray)
              .frame(width: 90, alignment: .trailing)
            Text(channel.name)
              .font(SpidolaType.body)
              .foregroundStyle(SpidolaPalette.broadcastWhite)
              .frame(maxWidth: .infinity, alignment: .leading)
            moveButton(channel: channel, direction: .up, disabled: index == 0) {
              Task { await model.moveUp(channel) }
            }
            moveButton(
              channel: channel, direction: .down, disabled: index == channels.count - 1
            ) { Task { await model.moveDown(channel) } }
          }
          .padding(.horizontal, SpidolaSpacing.l)
          .padding(.vertical, SpidolaSpacing.m)
          .background(SpidolaPalette.set)
          .accessibilityElement(children: .contain)
        }
      }
      .padding(.horizontal, SpidolaSpacing.safeHorizontal)
      .padding(.vertical, SpidolaSpacing.safeVertical)
    }
    .onAppear {
      guard focused == nil, channels.count > 1 else { return }
      focused = Focus(channelId: channels[0].id, direction: .down)
    }
  }

  private func moveButton(
    channel: PlayableChannel, direction: Direction, disabled: Bool, action: @escaping () -> Void
  ) -> some View {
    let title =
      direction == .up
      ? String(localized: "Move up", bundle: .module)
      : String(localized: "Move down", bundle: .module)
    let symbol = direction == .up ? "arrow.up" : "arrow.down"
    let target = Focus(channelId: channel.id, direction: direction)
    return Button(action: action) { Label(title, systemImage: symbol) }
      .buttonStyle(.plain)
      .font(SpidolaType.caption)
      .foregroundStyle(disabled ? SpidolaPalette.staticGray : SpidolaPalette.broadcastWhite)
      .padding(.horizontal, SpidolaSpacing.m)
      .padding(.vertical, SpidolaSpacing.s)
      .focused($focused, equals: target)
      .spidolaFocusRing(isFocused: focused == target)
      .disabled(disabled)
      .accessibilityLabel("\(title), \(channel.name)")
  }

  private struct Focus: Hashable {
    let channelId: String
    let direction: Direction
  }

  private enum Direction: Hashable { case up, down }
}
