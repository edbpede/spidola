// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import DesignSystem
import SwiftUI

/// Renders the generated notices shipped in the application bundle; it never embeds a stale copy.
public struct AboutView: View {
  private let version: String
  private let notices: String

  public init(version: String, notices: String) {
    self.version = version
    self.notices = notices
  }

  public var body: some View {
    ScrollView {
      VStack(alignment: .leading, spacing: SpidolaSpacing.l) {
        Text(String(localized: "Spidola \(version)", bundle: .module))
          .font(SpidolaType.display)
          .foregroundStyle(SpidolaPalette.broadcastWhite)
        Text(String(localized: "Third-party notices", bundle: .module))
          .font(SpidolaType.title)
          .foregroundStyle(SpidolaPalette.broadcastWhite)
        Text(
          notices.isEmpty
            ? String(localized: "Notices are unavailable in this build.", bundle: .module)
            : notices
        )
        .font(SpidolaType.caption)
        .foregroundStyle(SpidolaPalette.staticGray)
      }
      .padding(.horizontal, SpidolaSpacing.safeHorizontal)
      .padding(.vertical, SpidolaSpacing.safeVertical)
    }
    .frame(maxWidth: .infinity, maxHeight: .infinity)
    .background(SpidolaPalette.studio)
    .navigationTitle(String(localized: "About", bundle: .module))
  }
}
