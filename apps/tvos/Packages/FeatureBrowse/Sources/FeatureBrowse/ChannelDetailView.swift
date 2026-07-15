// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import CoreKit
import DesignSystem
import SwiftUI
import core_api

/// The channel detail screen: artwork, name, group, and the actions a household member reaches for
/// — Play, favorite, and hide.
public struct ChannelDetailView: View {
  @State private var model: ChannelDetailModel
  /// Play is a navigation intent, so the slice announces it and the shell decides where it goes —
  /// which keeps this screen free of the app's route enum and of the playback slice (doctrine §3.1).
  private let onPlay: @MainActor () -> Void

  @FocusState private var focused: Action?

  public init(
    channel: PlayableChannel, access: any BrowseAccess, onPlay: @escaping @MainActor () -> Void
  ) {
    _model = State(initialValue: ChannelDetailModel(channel: channel, access: access))
    self.onPlay = onPlay
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
      actionButton(
        .play, title: String(localized: "Play", bundle: .module), isPrimary: true, perform: onPlay)
      actionButton(
        .favorite,
        title: model.isFavorite
          ? String(localized: "Remove favorite", bundle: .module)
          : String(localized: "Add favorite", bundle: .module),
        value: model.isFavorite
          ? String(localized: "Favorite", bundle: .module)
          : String(localized: "Not a favorite", bundle: .module)
      ) { Task { await model.toggleFavorite() } }
      actionButton(
        .hide,
        title: model.isHidden
          ? String(localized: "Unhide", bundle: .module)
          : String(localized: "Hide", bundle: .module),
        value: model.isHidden
          ? String(localized: "Hidden", bundle: .module)
          : String(localized: "Visible", bundle: .module)
      ) {
        Task { await model.toggleHidden() }
      }
    }
    .padding(.top, SpidolaSpacing.s)
  }

  /// `value` is where the two buttons that toggle something say where the channel currently stands.
  /// Their titles only imply it: "Remove favorite" tells you this *is* a favorite by inference, and
  /// running a verb backwards is not something a listener should have to do to learn a fact the
  /// screen states plainly. Play has no state, so it has no value.
  private func actionButton(
    _ action: Action, title: String, value: String = "", isPrimary: Bool = false,
    perform: @escaping () -> Void
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
      .accessibilityValue(value)
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
