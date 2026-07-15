// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import CoreKit
import core_api

/// The presentation layer for the typed vocabulary the sources slice renders, resolved through this
/// package's catalog so the slice is translatable (PRD §6.10).
///
/// The words duplicate `CoreKit`'s `Source.kindLabel` rather than read it, for the same reason the
/// settings slice re-spells the buffering profiles (`SettingsOptions`): a label baked into a Swift
/// property is unreachable from a translation, and `CoreKit` carries no string catalog.
///
/// Two of the three keys are `AddSourceMode.title`'s own. That is the point: the add picker offers
/// "Playlist URL" and the list then describes the source it made as "Playlist URL", and a viewer who
/// meets the same words twice should meet the same translation twice. Sharing the key is what makes
/// that true by construction rather than by two people happening to type the same sentence.
///
/// The `@unknown default` arm reserves the "unknown future variant" case the boundary's enum is
/// declared to allow (TECH_SPEC §5), matching the property it mirrors.
extension Source {
  /// A couch-legible one-word description of the source kind, for the manage-sources list.
  var localizedKindLabel: String {
    switch self {
    case .m3uUrl: String(localized: "Playlist URL", bundle: .module)
    case .m3uFile: String(localized: "Playlist file", bundle: .module)
    case .xtream: String(localized: "Xtream account", bundle: .module)
    @unknown default: String(localized: "Source", bundle: .module)
    }
  }
}
