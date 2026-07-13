// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import CoreGraphics

/// The spacing scale and TV-safe insets (PRD §8.4: "All content respects TV-safe margins"). tvOS
/// supplies an overscan-safe area of its own; these are the content margins layered on top.
public enum SpidolaSpacing {
  public static let xs: CGFloat = 4
  public static let s: CGFloat = 8
  public static let m: CGFloat = 16
  public static let l: CGFloat = 24
  public static let xl: CGFloat = 48

  /// Horizontal content inset (PRD §8.4).
  public static let safeHorizontal: CGFloat = 48

  /// Vertical content inset (PRD §8.4).
  public static let safeVertical: CGFloat = 27
}
