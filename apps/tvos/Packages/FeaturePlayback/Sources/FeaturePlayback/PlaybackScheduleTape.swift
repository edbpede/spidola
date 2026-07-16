// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import DesignSystem
import Foundation
import SwiftUI
import core_api

/// The channel strip's fixed two-line now/next tape. It owns no loading work, so summoning the
/// strip remains a one-frame operation while guide content can arrive independently.
struct PlaybackScheduleTape: View {
  let nowNext: NowNext

  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  var body: some View {
    VStack(alignment: .leading, spacing: SpidolaSpacing.xs) {
      if nowNext.current == nil && nowNext.next == nil {
        unavailable
        unavailable.hidden()
      } else {
        programme(
          nowNext.current, marker: String(localized: "Now", bundle: .module), isCurrent: true,
          fallback: String(localized: "No programme now", bundle: .module))
        programme(
          nowNext.next, marker: String(localized: "Next", bundle: .module), isCurrent: false,
          fallback: String(localized: "No programme next", bundle: .module))
      }
    }
    .animation(
      reduceMotion ? nil : .easeInOut(duration: Self.transitionDuration), value: nowNext)
  }

  private var unavailable: some View {
    Text(String(localized: "Schedule unavailable", bundle: .module))
      .font(SpidolaType.caption)
      .foregroundStyle(SpidolaPalette.staticGray)
      .lineLimit(1)
  }

  private func programme(
    _ item: EpgProgramme?, marker: String, isCurrent: Bool, fallback: String
  ) -> some View {
    HStack(alignment: .firstTextBaseline, spacing: SpidolaSpacing.s) {
      Text(marker)
        .font(SpidolaType.caption)
        .foregroundStyle(isCurrent ? SpidolaPalette.testCardAmber : SpidolaPalette.staticGray)
        .frame(width: Self.markerWidth, alignment: .leading)
      Text(item.map { Self.time($0.startUnix) } ?? "—")
        .font(SpidolaType.caption)
        .monospacedDigit()
        .foregroundStyle(SpidolaPalette.staticGray)
        .frame(width: Self.timeWidth, alignment: .leading)
      Text(item?.title ?? fallback)
        .font(SpidolaType.caption)
        .foregroundStyle(item == nil ? SpidolaPalette.staticGray : SpidolaPalette.broadcastWhite)
        .lineLimit(1)
    }
  }

  private static func time(_ unix: Int64) -> String {
    Date(timeIntervalSince1970: TimeInterval(unix)).formatted(
      date: .omitted, time: .shortened)
  }

  private static let markerWidth: CGFloat = 68
  private static let timeWidth: CGFloat = 92
  private static let transitionDuration = 0.16
}
