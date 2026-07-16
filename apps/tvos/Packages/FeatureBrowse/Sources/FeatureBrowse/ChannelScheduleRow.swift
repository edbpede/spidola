// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import DesignSystem
import Foundation
import SwiftUI
import core_api

/// A fixed-height channel row with a two-line schedule tape. Schedule data can arrive while the
/// row owns focus without changing its identity or geometry.
struct ChannelScheduleRow: View {
  let row: ChannelRow
  let isFocused: Bool
  let action: () -> Void

  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  var body: some View {
    Button(action: action) {
      HStack(spacing: SpidolaSpacing.m) {
        LogoImage(url: row.channel.logo)
          .frame(width: Self.logoWidth, height: Self.logoHeight)
          .background(SpidolaPalette.studio)

        VStack(alignment: .leading, spacing: SpidolaSpacing.xs) {
          Text(row.channel.name)
            .font(SpidolaType.body)
            .foregroundStyle(SpidolaPalette.broadcastWhite)
            .lineLimit(1)
          Text(row.channel.groupTitle ?? String(localized: "Ungrouped", bundle: .module))
            .font(SpidolaType.caption)
            .foregroundStyle(SpidolaPalette.staticGray)
            .lineLimit(1)
        }

        Spacer(minLength: SpidolaSpacing.m)

        if row.isFavorite {
          Image(systemName: "star.fill")
            .font(.system(size: 26))
            .foregroundStyle(SpidolaPalette.testCardAmber)
            .accessibilityHidden(true)
        }

        schedule
          .frame(width: Self.scheduleWidth, alignment: .leading)
      }
      .frame(maxWidth: .infinity, minHeight: Self.rowHeight, alignment: .leading)
      .padding(.horizontal, SpidolaSpacing.m)
      .background(SpidolaPalette.set)
    }
    .buttonStyle(.plain)
    .spidolaFocusRing(isFocused: isFocused)
  }

  private var schedule: some View {
    VStack(alignment: .leading, spacing: SpidolaSpacing.xs) {
      switch row.schedule {
      case .pending, .unavailable:
        unavailableLine
        unavailableLine.hidden()
      case .ready(let nowNext):
        programmeLine(
          nowNext.current,
          fallback: String(localized: "No programme now", bundle: .module))
        programmeLine(
          nowNext.next,
          fallback: String(localized: "No programme next", bundle: .module))
      }
      progress
    }
    .animation(
      reduceMotion ? nil : .easeInOut(duration: Self.scheduleTransitionDuration),
      value: row.schedule)
  }

  private var unavailableLine: some View {
    Text(String(localized: "Schedule unavailable", bundle: .module))
      .font(SpidolaType.caption)
      .foregroundStyle(SpidolaPalette.staticGray)
      .lineLimit(1)
  }

  private func programmeLine(_ programme: EpgProgramme?, fallback: String) -> some View {
    HStack(alignment: .firstTextBaseline, spacing: SpidolaSpacing.s) {
      Text(programme.map { Self.time($0.startUnix) } ?? "—")
        .font(SpidolaType.caption)
        .monospacedDigit()
        .foregroundStyle(SpidolaPalette.staticGray)
        .frame(width: Self.timeWidth, alignment: .leading)
      Text(programme?.title ?? fallback)
        .font(SpidolaType.caption)
        .foregroundStyle(
          programme == nil ? SpidolaPalette.staticGray : SpidolaPalette.broadcastWhite
        )
        .lineLimit(1)
    }
  }

  @ViewBuilder private var progress: some View {
    if case .ready(let nowNext) = row.schedule, let current = nowNext.current {
      GeometryReader { proxy in
        ZStack(alignment: .leading) {
          Rectangle().fill(SpidolaPalette.staticGray.opacity(0.28))
          if isFocused {
            Rectangle()
              .fill(SpidolaPalette.testCardAmber)
              .frame(width: proxy.size.width * Self.progress(current, at: .now))
          }
        }
      }
      .frame(height: Self.progressHeight)
      .accessibilityHidden(true)
    } else {
      Color.clear.frame(height: Self.progressHeight)
    }
  }

  private static func time(_ unix: Int64) -> String {
    Date(timeIntervalSince1970: TimeInterval(unix)).formatted(
      date: .omitted, time: .shortened)
  }

  private static func progress(_ programme: EpgProgramme, at now: Date) -> CGFloat {
    let duration = max(1, programme.endUnix - programme.startUnix)
    let elapsed = Int64(now.timeIntervalSince1970) - programme.startUnix
    return min(1, max(0, CGFloat(elapsed) / CGFloat(duration)))
  }

  private static let logoWidth: CGFloat = 128
  private static let logoHeight: CGFloat = 72
  private static let rowHeight: CGFloat = 112
  private static let scheduleWidth: CGFloat = 620
  private static let timeWidth: CGFloat = 92
  private static let progressHeight: CGFloat = 3
  private static let scheduleTransitionDuration = 0.16
}
