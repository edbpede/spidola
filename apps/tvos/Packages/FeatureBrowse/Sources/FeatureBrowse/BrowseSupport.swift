// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import CoreKit
import DesignSystem
import SwiftUI
import core_api

/// The browse slice's navigation intents, wired by the app's composition root to the state-driven
/// `NavigationStack` (TECH_SPEC §6). The slice depends on these closures, never on the app's typed
/// route enum, so it stays free of sideways/upward dependencies (doctrine §3.1).
@MainActor
public struct BrowseNavigator {
  public var openSource: (_ id: Int64, _ name: String) -> Void
  public var openChannels:
    (_ sourceId: Int64, _ kind: MediaKind, _ group: String?, _ title: String) -> Void
  /// Opens a channel *along with the ring it was chosen from* — the list and the channel's position
  /// in it, which is what D-pad up/down zaps through (PRD §8.4). Each caller names its own ring, so
  /// a channel opened from favourites zaps favourites and one opened from a category zaps that
  /// category. `offset` is the channel's absolute position in the ring, not its index within a
  /// loaded page: zapping resolves it against the core, so a page-relative value would land the
  /// viewer on a different channel.
  public var openChannel:
    (_ channel: PlayableChannel, _ context: ZapContext, _ offset: UInt32) -> Void
  public var openSearch: () -> Void
  public var manageSources: () -> Void
  public var openSettings: () -> Void

  public init(
    openSource: @escaping (Int64, String) -> Void,
    openChannels: @escaping (Int64, MediaKind, String?, String) -> Void,
    openChannel: @escaping (PlayableChannel, ZapContext, UInt32) -> Void,
    openSearch: @escaping () -> Void,
    manageSources: @escaping () -> Void,
    openSettings: @escaping () -> Void
  ) {
    self.openSource = openSource
    self.openChannels = openChannels
    self.openChannel = openChannel
    self.openSearch = openSearch
    self.manageSources = manageSources
    self.openSettings = openSettings
  }
}

extension View {
  /// Renders a CoreKit `ActionableError` through the DesignSystem `ActionableErrorView`, wiring each
  /// prescribed action to a handler. The bridge lives at the feature layer because it joins the
  /// core error model (CoreKit) to the visual component (DesignSystem), and neither horizontal layer
  /// should depend on the other.
  @MainActor
  func actionableError(
    _ error: ActionableError,
    retry: @escaping @MainActor () -> Void,
    goBack: @escaping @MainActor () -> Void,
    fixInput: (@MainActor () -> Void)? = nil
  ) -> some View {
    func button(_ action: ErrorAction) -> SpidolaErrorButton {
      switch action {
      case .retry: SpidolaErrorButton(title: action.label, action: retry)
      case .goBack: SpidolaErrorButton(title: action.label, action: goBack)
      case .fixInput: SpidolaErrorButton(title: action.label, action: fixInput ?? goBack)
      }
    }
    return ActionableErrorView(
      failureClass: error.failureClass,
      message: error.message,
      primary: button(error.primaryAction),
      others: error.otherActions.map(button))
  }
}
