// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import PlayerContract
import core_api

/// The shell-side option sets behind each closed-set setting, in the order they are offered.
///
/// One enum per setting, each `String`-backed so a choice survives the round trip to the picker
/// screen and back as its own stable id — the same shape `AutoRefreshOption` already uses in the
/// sources slice. Every `from(...)` converter is total: a core value this build does not recognize
/// resolves to the option the core itself defaults to, so an app downgraded past a newly-added
/// variant shows a sensible row instead of an empty one. That is the `@unknown default` arm the
/// boundary rules require (TECH_SPEC §5), doing real work rather than trapping.

/// Which engine plays by default, app-wide. `automatic` is the absence of an override, not a
/// third engine — the core stores `nil` and the selection policy falls through to the platform
/// default (TECH_SPEC §8).
enum DefaultEngineOption: String, CaseIterable, Sendable {
  case automatic
  case mpv
  case avPlayer

  /// The opaque engine key the core persists; `nil` clears the override.
  var engineKey: String? {
    switch self {
    case .automatic: nil
    case .mpv: EngineID.mpv.rawValue
    case .avPlayer: EngineID.avPlayer.rawValue
    }
  }

  /// Resolves a stored key to an option. An unrecognized key reads as `automatic` for the same
  /// reason `EngineSelection.resolve` ignores an unregistered override: a stale preference naming
  /// an engine this build cannot construct must not leave the viewer stranded.
  static func from(engineKey: String?) -> Self {
    guard let engineKey else { return .automatic }
    return allCases.first { $0.engineKey == engineKey } ?? .automatic
  }

  var label: String {
    switch self {
    case .automatic: String(localized: "Automatic", bundle: .module)
    case .mpv: String(localized: "MPV", bundle: .module)
    case .avPlayer: String(localized: "AVPlayer", bundle: .module)
    }
  }

  var detail: String {
    switch self {
    case .automatic:
      String(localized: "Let Spidola pick. This works for almost everything.", bundle: .module)
    case .mpv:
      String(localized: "Plays the widest range of channels.", bundle: .module)
    case .avPlayer:
      String(localized: "Apple's player. Smoothest on channels that support it.", bundle: .module)
    }
  }
}

/// How much the player buffers before it starts.
///
/// Three steps, not the two PRD §6.9 names: "low-latency vs. stable" is the core's own reading of
/// that line — a summary of the axis, not a claim it has two positions — and the vocabulary the
/// core settled on mirrors `PlayerContract`'s exactly, `balanced` middle included. Offering two
/// would leave the middle, which is the default every fresh install plays at, unnameable.
///
/// The labels say what the trade buys rather than what the buffer does, and reuse the wording of
/// `PlayerContract.BufferingProfile.label` — but not because anything else shows them. Nothing
/// does: the in-playback options sheet offers tracks and aspect only, `bufferingProfile()` is read
/// solely to build a `StreamRequest`, and `.label` currently has no consumer at all. This screen is
/// the first place a viewer meets this vocabulary. The wording is borrowed because the contract had
/// already chosen couch-legible names for these exact three values, and inventing a second set
/// would seed a seam that opens the day playback does surface them.
///
/// The words are restated here rather than read from `.label` because they have to go through the
/// string catalog and `.label` does not. That duplication is the one thing here that can still
/// drift, and no test can catch it — the two would merely disagree, not fail.
enum BufferingOption: String, CaseIterable, Sendable {
  case low
  case balanced
  case generous

  // Qualified: `PlayerContract` declares a `BufferingProfile` too, and this slice imports both.
  // That the name collides is the point — the core adopted the contract's vocabulary wholesale —
  // but the settings surface must write the *core's* enum, so it says which one it means.
  var profile: core_api.BufferingProfile {
    switch self {
    case .low: .low
    case .balanced: .balanced
    case .generous: .generous
    }
  }

  static func from(_ profile: core_api.BufferingProfile) -> Self {
    switch profile {
    case .low: .low
    case .balanced: .balanced
    case .generous: .generous
    @unknown default: .balanced
    }
  }

  var label: String {
    switch self {
    case .low: String(localized: "Fastest start", bundle: .module)
    case .balanced: String(localized: "Balanced", bundle: .module)
    case .generous: String(localized: "Smoothest playback", bundle: .module)
    }
  }

  var detail: String {
    switch self {
    case .low:
      String(
        localized: "Starts quickest and stays closest to live. Needs a steady connection.",
        bundle: .module)
    case .balanced:
      String(localized: "The usual trade-off. Leave this on.", bundle: .module)
    case .generous:
      String(
        localized: "Rides out a patchy connection. Takes a moment longer to start.",
        bundle: .module)
    }
  }
}

/// How large subtitle text is drawn.
enum SubtitleSizeOption: String, CaseIterable, Sendable {
  case small
  case medium
  case large

  var size: SubtitleSize {
    switch self {
    case .small: .small
    case .medium: .medium
    case .large: .large
    }
  }

  static func from(_ size: SubtitleSize) -> Self {
    switch size {
    case .small: .small
    case .medium: .medium
    case .large: .large
    @unknown default: .medium
    }
  }

  var label: String {
    switch self {
    case .small: String(localized: "Small", bundle: .module)
    case .medium: String(localized: "Medium", bundle: .module)
    case .large: String(localized: "Large", bundle: .module)
    }
  }
}

/// What sits behind subtitle text so it stays readable over bright video.
enum SubtitleBackgroundOption: String, CaseIterable, Sendable {
  case none
  case shadow
  case solid

  var background: SubtitleBackground {
    switch self {
    case .none: .none
    case .shadow: .shadow
    case .solid: .solid
    }
  }

  static func from(_ background: SubtitleBackground) -> Self {
    switch background {
    case .none: .none
    case .shadow: .shadow
    case .solid: .solid
    @unknown default: .shadow
    }
  }

  var label: String {
    switch self {
    case .none: String(localized: "None", bundle: .module)
    case .shadow: String(localized: "Shadow", bundle: .module)
    case .solid: String(localized: "Solid", bundle: .module)
    }
  }

  var detail: String {
    switch self {
    case .none: String(localized: "Text only.", bundle: .module)
    case .shadow: String(localized: "A soft shadow. Readable on most pictures.", bundle: .module)
    case .solid: String(localized: "A solid block behind the words.", bundle: .module)
    }
  }
}

/// Which language the app is written in. Only tags that actually ship are offered: an option
/// naming a language with no strings behind it would be a promise the app cannot keep (PRD §6.10
/// ships English-first, with translations invited post-1.0).
enum LanguageOption: String, CaseIterable, Sendable {
  case system
  case english

  /// The BCP-47 tag the core persists; `nil` follows the system language.
  var tag: String? {
    switch self {
    case .system: nil
    case .english: "en"
    }
  }

  static func from(tag: String?) -> Self {
    guard let tag else { return .system }
    return allCases.first { $0.tag == tag } ?? .system
  }

  var label: String {
    switch self {
    case .system: String(localized: "System language", bundle: .module)
    case .english: String(localized: "English", bundle: .module)
    }
  }
}

/// How much breathing room rows get.
enum DensityOption: String, CaseIterable, Sendable {
  case comfortable
  case compact

  var density: InterfaceDensity {
    switch self {
    case .comfortable: .comfortable
    case .compact: .compact
    }
  }

  static func from(_ density: InterfaceDensity) -> Self {
    switch density {
    case .comfortable: .comfortable
    case .compact: .compact
    @unknown default: .comfortable
    }
  }

  var label: String {
    switch self {
    case .comfortable: String(localized: "Comfortable", bundle: .module)
    case .compact: String(localized: "Compact", bundle: .module)
    }
  }

  var detail: String {
    switch self {
    case .comfortable: String(localized: "Bigger rows, easier to read.", bundle: .module)
    case .compact: String(localized: "More on screen at once.", bundle: .module)
    }
  }
}

/// How long recently-watched history is kept.
enum RetentionOption: String, CaseIterable, Sendable {
  case month
  case quarter
  case year

  var days: UInt32 {
    switch self {
    case .month: 30
    case .quarter: 90
    case .year: 365
    }
  }

  static func from(days: UInt32) -> Self? {
    allCases.first { $0.days == days }
  }

  var label: String { RetentionOption.dayCountLabel(days) }

  /// The couch-legible spelling of a day count, used both for the options and for whatever value
  /// is actually stored — which is why it takes a count rather than reading `self`.
  ///
  /// Widened to `Int` before interpolating so the extracted key is a plain `%lld`, and pluralised
  /// through the catalog: none of the offered values is 1, but a value stored by another device
  /// could be, and "1 days" is the kind of seam that makes an app feel machine-made.
  static func dayCountLabel(_ days: UInt32) -> String {
    String(localized: "\(Int(days)) days", bundle: .module)
  }
}

/// The disk ceiling for cached artwork.
enum ImageCacheOption: String, CaseIterable, Sendable {
  case small
  case medium
  case large

  var megabytes: UInt32 {
    switch self {
    case .small: 128
    case .medium: 256
    case .large: 512
    }
  }

  static func from(megabytes: UInt32) -> Self? {
    allCases.first { $0.megabytes == megabytes }
  }

  var label: String { ImageCacheOption.megabyteLabel(megabytes) }

  /// The couch-legible spelling of a size, used for the options and for a stored value that is
  /// none of them. MB is a unit, not a count, so it does not pluralise.
  static func megabyteLabel(_ megabytes: UInt32) -> String {
    String(localized: "\(Int(megabytes)) MB", bundle: .module)
  }
}

/// How much detail the app records about what it is doing.
enum LogLevelOption: String, CaseIterable, Sendable {
  case error
  case warn
  case info
  case debug
  case trace

  var level: LogLevel {
    switch self {
    case .error: .error
    case .warn: .warn
    case .info: .info
    case .debug: .debug
    case .trace: .trace
    }
  }

  static func from(_ level: LogLevel) -> Self {
    switch level {
    case .error: .error
    case .warn: .warn
    case .info: .info
    case .debug: .debug
    case .trace: .trace
    @unknown default: .info
    }
  }

  var label: String {
    switch self {
    case .error: String(localized: "Errors only", bundle: .module)
    case .warn: String(localized: "Warnings", bundle: .module)
    case .info: String(localized: "Normal", bundle: .module)
    case .debug: String(localized: "Detailed", bundle: .module)
    case .trace: String(localized: "Everything", bundle: .module)
    }
  }

  var detail: String {
    switch self {
    case .error: String(localized: "Only record what went wrong.", bundle: .module)
    case .warn: String(localized: "Record problems and near-misses.", bundle: .module)
    case .info: String(localized: "The usual amount. Leave this on.", bundle: .module)
    case .debug: String(localized: "Extra detail, for chasing a problem.", bundle: .module)
    case .trace: String(localized: "Record everything. Slows the app down.", bundle: .module)
    }
  }
}
