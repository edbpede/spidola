// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import SwiftUI

/// The SMPTE colour-bar spectrum, tuned into Spidola's tonal family.
///
/// The bars are the classic seven, in broadcast order (white → yellow → cyan → green → magenta →
/// red → blue). Three of them are palette values the app already owns: the yellow bar **is**
/// `testCardAmber`, which is where the accent came from in the first place (PRD §8.2), and red/green
/// are the stream-health pair. The remaining four are derived to sit at the same saturation and
/// luminance, so the ribbon reads as one muted band rather than a strip of toy primaries on a
/// near-black canvas.
///
/// This is the only decorative element in the app (PRD §8.5). It earns its place by explaining the
/// accent: the viewer sees where Test-Card Amber comes from every time the strip appears.
public enum SmpteBars {
  /// The bars in broadcast left-to-right order.
  public static let ordered: [Color] = [
    SpidolaPalette.broadcastWhite,
    SpidolaPalette.testCardAmber,
    cyan,
    SpidolaPalette.streamGreen,
    magenta,
    SpidolaPalette.streamRed,
    blue,
  ]

  static let cyan = rgb(0x5F_A8_A8)
  static let magenta = rgb(0xA2_61_8F)
  static let blue = rgb(0x4E_6A_9E)

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

/// The three-pixel SMPTE ribbon that underlines the channel strip (PRD §8.5).
///
/// Deliberately not focusable and not announced to VoiceOver: it is decoration, and a screen reader
/// stopping on a colour bar would be noise between the channel name and the zap controls.
public struct SmpteRibbon: View {
  /// Three points at 10 feet is a hairline that reads as a broadcast artefact rather than a border.
  public static let height: CGFloat = 3

  public init() {}

  public var body: some View {
    HStack(spacing: 0) {
      ForEach(Array(SmpteBars.ordered.enumerated()), id: \.offset) { _, bar in
        Rectangle().fill(bar)
      }
    }
    .frame(height: Self.height)
    .accessibilityHidden(true)
  }
}
