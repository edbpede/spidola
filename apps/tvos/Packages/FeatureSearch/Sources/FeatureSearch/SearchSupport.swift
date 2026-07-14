// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import CoreKit
import DesignSystem
import SwiftUI

extension View {
  /// Renders a CoreKit `ActionableError` through the DesignSystem `ActionableErrorView`, wiring each
  /// prescribed action to a handler (PRD §6.3). Owned by the slice because it joins the core error
  /// model to the visual component, which neither horizontal layer depends on.
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
