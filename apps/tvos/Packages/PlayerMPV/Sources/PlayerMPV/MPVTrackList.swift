// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import Foundation
import PlayerContract

/// mpv's `track-list` property, decoded.
///
/// The property is read as a JSON string rather than walked as an `mpv_node` tree. Both are
/// supported by libmpv, and JSON wins on the axis that matters: the mapping becomes a pure
/// `String -> TrackSelection` function that a unit test can drive with a fixture, whereas an
/// `mpv_node` walk would need a live core to produce its input and hand-rolled pointer traversal
/// to consume it. The cost is one JSON parse per track-list change — an event that fires once per
/// load, not per frame.
enum MPVTrackList {

  /// One entry of mpv's `track-list`. Only the fields the contract's `MediaTrack` needs are
  /// declared; `Codable` ignores the rest of mpv's much wider entry shape.
  ///
  /// No `CodingKeys`: every property already matches mpv's JSON key exactly, so the synthesized
  /// conformance is the correct one. Spelling them out again would be a second copy of the same
  /// list, free to drift from the first.
  private struct Entry: Decodable {
    let id: Int
    let type: String
    let title: String?
    let lang: String?
    let selected: Bool?
    let codec: String?
  }

  /// Parses mpv's `track-list` JSON into the contract's track menu.
  ///
  /// Returns an empty selection for malformed input rather than throwing: a track menu is a
  /// convenience surface, and a stream whose track list we cannot read should still play with its
  /// default tracks. Failing the whole load over an unreadable menu would trade a working channel
  /// for a pedantic error.
  static func parse(json: String) -> TrackSelection {
    guard let data = json.data(using: .utf8),
      let entries = try? JSONDecoder().decode([Entry].self, from: data)
    else {
      return TrackSelection()
    }

    var available: [MediaTrack] = []
    var selectedAudio: TrackID?
    var selectedSubtitle: TrackID?

    for entry in entries {
      guard let kind = kind(forMPVType: entry.type) else { continue }
      let id = TrackID(rawValue: String(entry.id))
      available.append(
        MediaTrack(id: id, kind: kind, label: label(for: entry), language: entry.lang))

      guard entry.selected == true else { continue }
      switch kind {
      case .audio: selectedAudio = id
      case .subtitle: selectedSubtitle = id
      }
    }

    return TrackSelection(
      available: available, selectedAudio: selectedAudio, selectedSubtitle: selectedSubtitle)
  }

  /// mpv's track type mapped onto the contract's kinds. `video` returns `nil` — the contract has
  /// no video-track arm, by its own design note, so those entries are dropped rather than forced.
  private static func kind(forMPVType type: String) -> TrackKind? {
    switch type {
    case "audio": .audio
    case "sub": .subtitle
    default: nil
    }
  }

  /// The label the track menu shows.
  ///
  /// mpv leaves `title` nil for the majority of IPTV streams, which declare a language and nothing
  /// else. The fallbacks walk down what the stream actually gave us — a title, then a language, then
  /// the codec — so the menu reads "English" or "AAC" instead of the untranslatable "Track 2".
  /// `EngineError`-style couch language is not applied here because these are the stream's own
  /// words, not ours to rewrite.
  ///
  /// A stream that declares none of the three leaves us with nothing to report, and an empty label
  /// says exactly that. Naming that track is the playback slice's job: the words would have to be
  /// translated and this package has no catalog, and a name invented here from an mpv track id
  /// would read differently on the other engine for the same stream (TECH_SPEC §8).
  private static func label(for entry: Entry) -> String {
    if let title = entry.title, !title.isEmpty { return title }
    if let lang = entry.lang, !lang.isEmpty { return lang }
    if let codec = entry.codec, !codec.isEmpty { return codec.uppercased() }
    return ""
  }
}
