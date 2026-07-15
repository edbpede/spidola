// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import CoreKit
import core_api

/// The presentation layer for the typed vocabulary the browse slice renders, resolved through this
/// package's catalog so the slice is translatable (PRD §6.10).
///
/// The words duplicate `CoreKit`'s `MediaKind.label` and `Source.kindLabel` rather than read them,
/// for the same reason the settings slice re-spells the buffering profiles (`SettingsOptions`): a
/// label baked into a Swift property is unreachable from a translation, and `CoreKit` carries no
/// string catalog — nor should it, since the words belong to the slice that puts them on screen and
/// not to the boundary that names the values.
///
/// The `@unknown default` arms reserve the "unknown future variant" case the boundary's enums are
/// declared to allow (TECH_SPEC §5), matching the properties they mirror.
extension MediaKind {
  /// A couch-legible label for the "type" level of the browse drill-down.
  var localizedLabel: String {
    switch self {
    case .live: String(localized: "Live", bundle: .module)
    case .movie: String(localized: "Movies", bundle: .module)
    case .seriesEpisode: String(localized: "Series", bundle: .module)
    @unknown default: String(localized: "Channels", bundle: .module)
    }
  }
}

extension Source {
  /// A couch-legible one-word description of the source kind, for the home screen's source list.
  var localizedKindLabel: String {
    switch self {
    case .m3uUrl: String(localized: "Playlist URL", bundle: .module)
    case .m3uFile: String(localized: "Playlist file", bundle: .module)
    case .xtream: String(localized: "Xtream account", bundle: .module)
    @unknown default: String(localized: "Source", bundle: .module)
    }
  }
}
