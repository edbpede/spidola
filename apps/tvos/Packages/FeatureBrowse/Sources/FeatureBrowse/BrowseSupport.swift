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
  public var openChannel: (PlayableChannel) -> Void
  public var openSearch: () -> Void
  public var manageSources: () -> Void

  public init(
    openSource: @escaping (Int64, String) -> Void,
    openChannels: @escaping (Int64, MediaKind, String?, String) -> Void,
    openChannel: @escaping (PlayableChannel) -> Void,
    openSearch: @escaping () -> Void,
    manageSources: @escaping () -> Void
  ) {
    self.openSource = openSource
    self.openChannels = openChannels
    self.openChannel = openChannel
    self.openSearch = openSearch
    self.manageSources = manageSources
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
