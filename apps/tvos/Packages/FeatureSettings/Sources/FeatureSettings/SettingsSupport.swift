// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import CoreKit
import DesignSystem
import SwiftUI
import core_api

/// The settings slice's navigation intents, wired by the app's composition root to the state-driven
/// `NavigationStack` (TECH_SPEC §6). The slice depends on these closures, never on the app's typed
/// route enum, so it stays free of sideways/upward dependencies (doctrine §3.1).
@MainActor
public struct SettingsNavigator {
  /// Opens the picker for one closed-set setting. The root and the diagnostics screen share it —
  /// there is one picker screen in this slice, and the field it is about is its whole payload.
  public var openOptions: (_ field: SettingsField) -> Void
  public var openDiagnostics: () -> Void
  public var openAbout: () -> Void

  public init(
    openOptions: @escaping (SettingsField) -> Void,
    openDiagnostics: @escaping () -> Void,
    openAbout: @escaping () -> Void
  ) {
    self.openOptions = openOptions
    self.openDiagnostics = openDiagnostics
    self.openAbout = openAbout
  }
}

extension View {
  /// Renders a CoreKit `ActionableError` through the DesignSystem `ActionableErrorView`, wiring each
  /// prescribed action to a handler. The bridge lives at the feature layer because it joins the
  /// core error model (CoreKit) to the visual component (DesignSystem), and neither horizontal layer
  /// should depend on the other — which is also why each slice carries its own copy rather than
  /// importing a neighbour's.
  @MainActor
  func actionableError(
    _ error: ActionableError,
    retry: @escaping @MainActor () -> Void,
    goBack: @escaping @MainActor () -> Void
  ) -> some View {
    func button(_ action: ErrorAction) -> SpidolaErrorButton {
      switch action {
      case .retry: SpidolaErrorButton(title: action.label, action: retry)
      case .goBack: SpidolaErrorButton(title: action.label, action: goBack)
      // Nothing on a settings screen is typed in, so there is no input to go back and correct;
      // re-reading is the nearest honest action.
      case .fixInput: SpidolaErrorButton(title: ErrorAction.retry.label, action: retry)
      }
    }
    return ActionableErrorView(
      failureClass: error.failureClass,
      message: error.message,
      primary: button(error.primaryAction),
      others: error.otherActions.map(button))
  }
}
