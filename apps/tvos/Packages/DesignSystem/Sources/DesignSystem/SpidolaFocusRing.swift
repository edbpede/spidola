// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import SwiftUI

/// The Test-Card Amber focus treatment (PRD §8.4: "the focused element is always unmistakable —
/// scale plus Test-Card Amber underline/border"). It rides the native tvOS focus engine rather
/// than fighting it: the caller reads focus with `@FocusState` and passes it in, and every
/// focusable gets the same amber border and lift. The animation stays under the reduce-motion
/// ceiling (all motion < 200 ms, PRD §8.6).
public struct SpidolaFocusRing: ViewModifier {
  private let isFocused: Bool
  private let cornerRadius: CGFloat = 12
  private let borderWidth: CGFloat = 3
  private let focusedScale: CGFloat = 1.05

  public init(isFocused: Bool) {
    self.isFocused = isFocused
  }

  public func body(content: Content) -> some View {
    content
      .overlay(
        RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
          .strokeBorder(SpidolaPalette.testCardAmber, lineWidth: isFocused ? borderWidth : 0)
      )
      .scaleEffect(isFocused ? focusedScale : 1)
      .animation(.easeOut(duration: 0.15), value: isFocused)
  }
}

extension View {
  /// Applies the Test-Card Amber focus ring driven by the caller's focus state.
  public func spidolaFocusRing(isFocused: Bool) -> some View {
    modifier(SpidolaFocusRing(isFocused: isFocused))
  }
}
