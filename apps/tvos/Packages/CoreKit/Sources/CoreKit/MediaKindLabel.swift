// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import core_api

extension MediaKind {
  /// A couch-legible label for the "type" level of the browse drill-down and the search filter.
  /// The `@unknown default` reserves the "unknown future variant" arm (TECH_SPEC §5).
  public var label: String {
    switch self {
    case .live: "Live"
    case .movie: "Movies"
    case .seriesEpisode: "Series"
    @unknown default: "Channels"
    }
  }
}
