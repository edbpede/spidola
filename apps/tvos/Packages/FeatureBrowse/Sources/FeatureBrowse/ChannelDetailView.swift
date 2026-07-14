// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import CoreKit
import DesignSystem
import SwiftUI
import core_api

/// The channel detail screen: artwork, name, group, and the actions a household member reaches for
/// — Play (records a recent; the engine lands in Phase 5), favorite, and hide.
public struct ChannelDetailView: View {
  @State private var model: ChannelDetailModel

  @FocusState private var focused: Action?

  public init(channel: PlayableChannel, access: any BrowseAccess) {
    _model = State(initialValue: ChannelDetailModel(channel: channel, access: access))
  }

  public var body: some View {
    let channel = model.channel
    HStack(alignment: .top, spacing: SpidolaSpacing.xl) {
      LogoImage(url: channel.logo)
        .frame(width: 420, height: 420 * 9 / 16)
        .background(SpidolaPalette.set)
      VStack(alignment: .leading, spacing: SpidolaSpacing.m) {
        Text(channel.name)
          .font(SpidolaType.display)
          .foregroundStyle(SpidolaPalette.broadcastWhite)
        if let group = channel.group {
          Text(group)
            .font(SpidolaType.body)
            .foregroundStyle(SpidolaPalette.staticGray)
        }
        Text(host(of: channel.locator))
          .font(SpidolaType.caption)
          .foregroundStyle(SpidolaPalette.staticGray)
        actions
        if let notice = model.notice {
          Text(notice)
            .font(SpidolaType.caption)
            .foregroundStyle(SpidolaPalette.testCardAmber)
        }
      }
      Spacer(minLength: 0)
    }
    .padding(SpidolaSpacing.xl)
    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    .background(SpidolaPalette.studio)
    .task { await model.load() }
    .onAppear { focused = .play }
  }

  private var actions: some View {
    HStack(spacing: SpidolaSpacing.m) {
      actionButton(.play, title: "Play", isPrimary: true) { Task { await model.play() } }
      actionButton(
        .favorite, title: model.isFavorite ? "Remove favorite" : "Add favorite"
      ) { Task { await model.toggleFavorite() } }
      actionButton(.hide, title: model.isHidden ? "Unhide" : "Hide") {
        Task { await model.toggleHidden() }
      }
    }
    .padding(.top, SpidolaSpacing.s)
  }

  private func actionButton(
    _ action: Action, title: String, isPrimary: Bool = false, perform: @escaping () -> Void
  ) -> some View {
    Button(title, action: perform)
      .buttonStyle(.plain)
      .padding(.horizontal, SpidolaSpacing.l)
      .padding(.vertical, SpidolaSpacing.m)
      .background(isPrimary ? SpidolaPalette.testCardAmber : SpidolaPalette.set)
      .foregroundStyle(isPrimary ? SpidolaPalette.studio : SpidolaPalette.broadcastWhite)
      .font(SpidolaType.body)
      .focused($focused, equals: action)
      .spidolaFocusRing(isFocused: focused == action)
      .accessibilityIdentifier("detail-\(action)")
  }

  private func host(of locator: String) -> String {
    URL(string: locator)?.host() ?? locator
  }

  private enum Action: Hashable, CustomStringConvertible {
    case play, favorite, hide
    var description: String {
      switch self {
      case .play: "play"
      case .favorite: "favorite"
      case .hide: "hide"
      }
    }
  }
}
