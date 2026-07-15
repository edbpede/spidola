// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import PlayerContract

/// The presentation layer for the typed vocabulary the playback slice renders, resolved through this
/// package's catalog so the slice is translatable (PRD §6.10).
///
/// The words duplicate `AspectMode`'s own `label` rather than read it, for the same reason the
/// settings slice re-spells the buffering profiles (`SettingsOptions`): a label baked into a
/// contract property is unreachable from a translation, and `PlayerContract` carries no string
/// catalog. The contract's job is to name the values every engine honours; naming them *to the
/// viewer* is this slice's.
extension AspectMode {
  /// The couch-legible label (PRD §8.6 voice).
  var localizedLabel: String {
    switch self {
    case .fit: String(localized: "Fit", bundle: .module)
    case .fill: String(localized: "Fill", bundle: .module)
    case .stretch: String(localized: "Stretch", bundle: .module)
    }
  }
}

extension MediaTrack {
  /// What to call a track the stream named in no way at all — no title, no language, no codec.
  ///
  /// The engines report an empty label for that track rather than inventing one, because the words
  /// are neither theirs to translate nor theirs to choose: the same nameless track must read the
  /// same whichever engine opened it (TECH_SPEC §8), and only this slice has a catalog to say it in.
  ///
  /// Numbered by position within its kind rather than by `TrackID`, whose whole reason for being a
  /// newtype is that each engine keeps its own numbering — mpv's track ids, AVFoundation's option
  /// indices — "without leaking it into the UI". Showing that raw is exactly the leak it names: one
  /// stream would be "Subtitle 7" on one engine and "Subtitle 2" on the other. Its position in the
  /// menu is the one number that means the same thing on both, and it is the number the viewer can
  /// actually see, being the row they are counting down to.
  func localizedFallbackLabel(position: Int) -> String {
    switch kind {
    case .audio: String(localized: "Audio \(position)", bundle: .module)
    case .subtitle: String(localized: "Subtitle \(position)", bundle: .module)
    }
  }
}
