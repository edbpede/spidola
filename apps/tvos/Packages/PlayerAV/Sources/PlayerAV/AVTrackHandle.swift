// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import AVFoundation
import PlayerContract

/// The addressing scheme that lets a contract `TrackID` name an `AVMediaSelectionOption`.
///
/// AVFoundation gives its selection options no usable public identifier: `displayName` is
/// localized and not unique (two "English" audio tracks are routine on a portal stream), and the
/// `propertyList` round-trip is an opaque blob with no string form. So a track handle has to be
/// built rather than read. This encodes the two coordinates that *do* address an option
/// unambiguously — which selection group it lives in, and its index within that group's `options`
/// array — as `"<group>:<index>"`, e.g. `"audible:0"`, `"legible:2"`.
///
/// Index stability is bought from the contract rather than assumed: `PlaybackEngine` states that
/// an engine is loaded once and then disposed, so the asset behind these indices cannot change
/// underneath a live handle. A rebuilt engine mints fresh handles against its own asset.
struct AVTrackHandle: Equatable, Hashable {
  /// The `AVMediaSelectionGroup` a handle addresses. Only the two the contract exposes: `TrackKind`
  /// has no video case, so neither does this.
  enum Group: String, CaseIterable {
    case audible
    case legible

    var characteristic: AVMediaCharacteristic {
      switch self {
      case .audible: .audible
      case .legible: .legible
      }
    }

    var kind: TrackKind {
      switch self {
      case .audible: .audio
      case .legible: .subtitle
      }
    }
  }

  let group: Group
  let optionIndex: Int

  init(group: Group, optionIndex: Int) {
    self.group = group
    self.optionIndex = optionIndex
  }

  /// Decodes a handle the UI is handing back.
  ///
  /// Returns `nil` for anything this engine did not mint — an mpv engine's `TrackID` that reached
  /// the wrong engine through a stale menu, or a malformed string — so `select(track:)` can
  /// no-op on a foreign value instead of trapping on it.
  init?(trackID: TrackID) {
    let parts = trackID.rawValue.split(
      separator: ":", maxSplits: 1, omittingEmptySubsequences: false)
    guard parts.count == 2,
      let group = Group(rawValue: String(parts[0])),
      let index = Int(parts[1]),
      index >= 0
    else { return nil }
    self.group = group
    self.optionIndex = index
  }

  var trackID: TrackID {
    TrackID(rawValue: "\(group.rawValue):\(optionIndex)")
  }
}
