// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import SwiftUI

/// The five named palette values from PRD §8.2, plus the two muted semantic colors reserved for
/// stream-health / error contexts. Test-Card Amber marks exactly focus, the live indicator, and
/// primary actions and appears nowhere else.
public enum SpidolaPalette {
  /// Canvas — a near-black with a cool cast; the base surface.
  public static let studio = rgb(0x12_15_1A)

  /// Raised surface for cards, rails, and overlays.
  public static let set = rgb(0x1C_21_29)

  /// Primary text — a warm paper-white that reads softly at 10 feet.
  public static let broadcastWhite = rgb(0xF1_EF_E9)

  /// Secondary text and inactive metadata.
  public static let staticGray = rgb(0x8B_94_A3)

  /// The single accent (SMPTE yellow bar): focus, the live indicator, primary actions only.
  public static let testCardAmber = rgb(0xE3_A4_4A)

  /// Stream-health / error only, muted into the same tonal family.
  public static let streamRed = rgb(0xC0_55_4E)

  /// Stream-health only, muted into the same tonal family.
  public static let streamGreen = rgb(0x6F_A3_6A)

  private static func rgb(_ hex: UInt32) -> Color {
    Color(
      .sRGB,
      red: Double((hex >> 16) & 0xFF) / 255,
      green: Double((hex >> 8) & 0xFF) / 255,
      blue: Double(hex & 0xFF) / 255,
      opacity: 1
    )
  }
}
