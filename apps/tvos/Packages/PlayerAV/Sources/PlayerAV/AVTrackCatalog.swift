// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import AVFoundation
import PlayerContract

/// An asset's audio and subtitle menus, rendered into the contract's vocabulary and resolvable
/// back into AVFoundation's.
///
/// It holds the loaded `AVMediaSelectionGroup`s because resolution needs them: an `AVTrackHandle`
/// addresses an option by index *within its group*, so `select` has to consult the same group
/// list `available` was built from. Loading them once, at `load` time, is also what lets
/// `select(track:)` stay synchronous the way the contract declares it — the alternative is an
/// `await` inside a call the UI makes from a button press.
@MainActor
struct AVTrackCatalog {
  private let groups: [AVTrackHandle.Group: AVMediaSelectionGroup]

  /// The catalog of an engine that has not loaded yet: no groups, so every lookup misses and
  /// every menu is empty.
  static let empty = AVTrackCatalog(groups: [:])

  /// Reads both selection groups off `asset`.
  ///
  /// A group that fails to load is absent rather than fatal: a stream with no subtitles at all
  /// is the common case, not an error, and a stream whose audible group fails to parse should
  /// still play with whatever AVPlayer picked by default. Either way the viewer gets a shorter
  /// menu, never a failed channel.
  static func load(from asset: AVAsset) async -> AVTrackCatalog {
    var loaded: [AVTrackHandle.Group: AVMediaSelectionGroup] = [:]
    for group in AVTrackHandle.Group.allCases {
      if let selection = try? await asset.loadMediaSelectionGroup(for: group.characteristic) {
        loaded[group] = selection
      }
    }
    return AVTrackCatalog(groups: loaded)
  }

  func group(_ group: AVTrackHandle.Group) -> AVMediaSelectionGroup? {
    groups[group]
  }

  /// The option a handle names, or `nil` when the handle addresses a group this asset does not
  /// have or an index past its end.
  func option(for handle: AVTrackHandle) -> AVMediaSelectionOption? {
    guard let group = groups[handle.group],
      group.options.indices.contains(handle.optionIndex)
    else { return nil }
    return group.options[handle.optionIndex]
  }

  /// The contract's track menu for `item`, including what is currently on.
  ///
  /// Selection is read from the item rather than tracked here, because AVPlayer selects a default
  /// audio option by itself the moment the asset loads — a menu that only knew about explicit
  /// `select` calls would show nothing selected while a track was audibly playing.
  func selection(in item: AVPlayerItem) -> TrackSelection {
    let current = item.currentMediaSelection
    var available: [MediaTrack] = []
    var selectedAudio: TrackID?
    var selectedSubtitle: TrackID?

    for group in AVTrackHandle.Group.allCases {
      guard let selectionGroup = groups[group] else { continue }
      let chosen = current.selectedMediaOption(in: selectionGroup)
      for (index, option) in selectionGroup.options.enumerated() {
        let handle = AVTrackHandle(group: group, optionIndex: index)
        available.append(
          MediaTrack(
            id: handle.trackID,
            kind: group.kind,
            label: option.displayName,
            language: option.extendedLanguageTag))
        guard option == chosen else { continue }
        switch group {
        case .audible: selectedAudio = handle.trackID
        case .legible: selectedSubtitle = handle.trackID
        }
      }
    }

    return TrackSelection(
      available: available, selectedAudio: selectedAudio, selectedSubtitle: selectedSubtitle)
  }
}
