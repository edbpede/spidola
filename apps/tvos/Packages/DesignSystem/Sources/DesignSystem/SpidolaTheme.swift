// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import SwiftUI

extension View {
  /// Applies the Spidola look (PRD §8): dark-first, the Studio canvas, and Test-Card Amber as the
  /// single accent (`tint`). Dark-first is a considered choice — the app lives on living-room
  /// panels in dim rooms over full-motion video; there is no light variant.
  public func spidolaTheme() -> some View {
    preferredColorScheme(.dark)
      .tint(SpidolaPalette.testCardAmber)
      .background(SpidolaPalette.studio.ignoresSafeArea())
  }
}
