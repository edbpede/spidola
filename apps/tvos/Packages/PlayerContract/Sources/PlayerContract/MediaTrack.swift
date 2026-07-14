// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

/// An engine-scoped track handle. A newtype rather than a bare `Int`/`String` so a subtitle id
/// can never be passed where an audio id belongs, and so each engine keeps its own numbering
/// (mpv track ids, AVFoundation option indices) without leaking it into the UI.
public struct TrackID: RawRepresentable, Sendable, Equatable, Hashable {
  public let rawValue: String

  public init(rawValue: String) {
    self.rawValue = rawValue
  }
}

/// Which stream a track belongs to. Video tracks are deliberately absent: no shipped surface
/// selects one, and an unused arm would be an untested arm.
public enum TrackKind: Sendable, Equatable, Hashable, CaseIterable {
  case audio
  case subtitle
}

/// One selectable audio or subtitle track, as the UI shows it.
public struct MediaTrack: Sendable, Equatable, Hashable, Identifiable {
  public let id: TrackID
  public let kind: TrackKind
  /// The engine's human label, already de-jargoned where the engine gives us the chance.
  public let label: String
  /// BCP-47-ish language tag when the stream declares one.
  public let language: String?

  public init(id: TrackID, kind: TrackKind, label: String, language: String? = nil) {
    self.id = id
    self.kind = kind
    self.label = label
    self.language = language
  }
}

/// The engine's current track menu: what exists and what is on. Selection is a separate field
/// rather than an `isSelected` flag per track, so "exactly one audio track is selected" is true
/// by construction instead of by convention.
public struct TrackSelection: Sendable, Equatable, Hashable {
  public let available: [MediaTrack]
  public let selectedAudio: TrackID?
  public let selectedSubtitle: TrackID?

  public init(
    available: [MediaTrack] = [],
    selectedAudio: TrackID? = nil,
    selectedSubtitle: TrackID? = nil
  ) {
    self.available = available
    self.selectedAudio = selectedAudio
    self.selectedSubtitle = selectedSubtitle
  }

  public func tracks(of kind: TrackKind) -> [MediaTrack] {
    available.filter { $0.kind == kind }
  }
}
