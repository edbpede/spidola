// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import CoreKit
import DesignSystem
import SwiftUI

/// A custom channel's lower-third. It intentionally has no adjacent peeks: custom navigation
/// carries one sealed channel handle, not a prefetched lineup or any stream request data.
struct CustomChannelStrip: View {
  let channel: CustomPlayableChannel

  var body: some View {
    VStack(alignment: .leading, spacing: 0) {
      HStack(spacing: SpidolaSpacing.m) {
        LogoImage(url: channel.logo)
          .frame(width: Self.logoWidth, height: Self.logoWidth * 9 / 16)
          .background(SpidolaPalette.studio)
        Text(channel.name)
          .font(SpidolaType.title)
          .foregroundStyle(SpidolaPalette.broadcastWhite)
          .lineLimit(1)
        Spacer(minLength: 0)
        LiveMarker()
      }
      .padding(.horizontal, SpidolaSpacing.xl)
      .padding(.vertical, SpidolaSpacing.m)
      SmpteRibbon()
    }
    .background(SpidolaPalette.set.opacity(0.92))
    .frame(maxWidth: .infinity, alignment: .leading)
    .accessibilityElement(children: .combine)
    .accessibilityLabel(channel.name)
    .accessibilityValue(String(localized: "Live", bundle: .module))
  }

  private static let logoWidth: CGFloat = 120
}
