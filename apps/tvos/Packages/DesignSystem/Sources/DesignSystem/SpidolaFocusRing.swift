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

  /// Honoured here rather than at each call site because this modifier *is* the app's focus
  /// motion: every focusable surface wears it, so reading the setting once is what makes
  /// "all motion suppressed under reduce-motion" (PRD §8.6, §6.10) true everywhere at once
  /// instead of true wherever someone remembered.
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  public init(isFocused: Bool) {
    self.isFocused = isFocused
  }

  public func body(content: Content) -> some View {
    content
      .overlay(
        RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
          .strokeBorder(SpidolaPalette.testCardAmber, lineWidth: isFocused ? borderWidth : 0)
      )
      // The amber border stays under reduce-motion; only the movement goes. Focus must remain
      // unmistakable (PRD §8.4) — suppressing the scale is not the same as hiding the ring, and a
      // viewer who turned motion off still has to see where they are.
      .scaleEffect(reduceMotion ? 1 : (isFocused ? focusedScale : 1))
      .animation(reduceMotion ? nil : .easeOut(duration: 0.15), value: isFocused)
  }
}

extension View {
  /// Applies the Test-Card Amber focus ring driven by the caller's focus state.
  public func spidolaFocusRing(isFocused: Bool) -> some View {
    modifier(SpidolaFocusRing(isFocused: isFocused))
  }
}
