// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import DesignSystem
import Foundation
import SwiftUI
import core_api

/// A compact broadcast schedule tape. Time remains textual; progress is supplemental only.
struct ScheduleTape: View {
  let nowNext: NowNext
  let upcoming: [EpgProgramme]
  let unavailable: Bool

  var body: some View {
    VStack(alignment: .leading, spacing: SpidolaSpacing.s) {
      Text(String(localized: "Programme guide", bundle: .module))
        .font(SpidolaType.caption)
        .foregroundStyle(SpidolaPalette.staticGray)

      if unavailable {
        Text(String(localized: "Schedule unavailable", bundle: .module))
          .font(SpidolaType.body)
          .foregroundStyle(SpidolaPalette.staticGray)
      } else {
        if let current = nowNext.current {
          programme(current, marker: String(localized: "Now", bundle: .module), isCurrent: true)
        }
        if let next = nowNext.next {
          programme(next, marker: String(localized: "Next", bundle: .module), isCurrent: false)
        }
        ForEach(laterProgrammes.prefix(2), id: \.id) { programme in
          self.programme(programme, marker: time(programme.startUnix), isCurrent: false)
        }
      }
    }
    .padding(SpidolaSpacing.m)
    .frame(maxWidth: 760, alignment: .leading)
    .background(SpidolaPalette.set)
    .accessibilityElement(children: .contain)
  }

  private var laterProgrammes: [EpgProgramme] {
    let visible = Set([nowNext.current?.id, nowNext.next?.id].compactMap { $0 })
    return upcoming.filter { !visible.contains($0.id) }
  }

  private func programme(_ item: EpgProgramme, marker: String, isCurrent: Bool) -> some View {
    HStack(alignment: .firstTextBaseline, spacing: SpidolaSpacing.m) {
      Text(marker)
        .font(SpidolaType.caption)
        .foregroundStyle(isCurrent ? SpidolaPalette.testCardAmber : SpidolaPalette.staticGray)
        .frame(width: 90, alignment: .leading)
      VStack(alignment: .leading, spacing: SpidolaSpacing.xs) {
        Text(item.title)
          .font(SpidolaType.body)
          .foregroundStyle(SpidolaPalette.broadcastWhite)
          .lineLimit(1)
        Text("\(time(item.startUnix))–\(time(item.endUnix))")
          .font(SpidolaType.caption)
          .foregroundStyle(SpidolaPalette.staticGray)
      }
    }
    .accessibilityElement(children: .combine)
  }

  private func time(_ unix: Int64) -> String {
    Date(timeIntervalSince1970: TimeInterval(unix)).formatted(date: .omitted, time: .shortened)
  }
}
