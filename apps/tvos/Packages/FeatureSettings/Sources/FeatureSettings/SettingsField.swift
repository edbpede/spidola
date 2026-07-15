// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import CoreKit
import core_api

/// One choice on a picker screen: a stable id, a couch-legible label, and an optional one-line
/// explanation of what picking it does.
public struct SettingsChoice: Identifiable, Hashable, Sendable {
  public let id: String
  public let label: String
  public let detail: String?

  init(id: String, label: String, detail: String? = nil) {
    self.id = id
    self.label = label
    self.detail = detail
  }
}

/// A setting whose values are a closed set — the rows that open a picker rather than acting in
/// place. It is the payload the app's route carries, which is why it is `Hashable` and spelled in
/// stable raw values rather than positions.
///
/// This enum is the single place the slice joins its option vocabulary to the core's: every field
/// answers what its row shows, what its picker offers, which option is currently marked, and how a
/// pick is written back. Adding a setting means adding a case and answering those four questions —
/// the screens below need no edit at all.
///
/// **The EPG window is deliberately not here.** The core's vocabulary has it (PRD §6.9 lists it),
/// but EPG ingest is Phase 8: a row that changed a number no viewer could observe would be a UX
/// bug, not a feature. It joins this enum when the EPG screens land and the window starts meaning
/// something.
public enum SettingsField: String, Hashable, Sendable, CaseIterable {
  case defaultEngine
  case buffering
  case subtitleSize
  case subtitleBackground
  case language
  case density
  case recentsRetention
  case imageCache
  case logLevel

  /// The row's title and the picker screen's heading — the setting's name.
  public var title: String {
    switch self {
    case .defaultEngine: String(localized: "Default player", bundle: .module)
    case .buffering: String(localized: "Buffering", bundle: .module)
    case .subtitleSize: String(localized: "Subtitle size", bundle: .module)
    case .subtitleBackground: String(localized: "Subtitle background", bundle: .module)
    case .language: String(localized: "Language", bundle: .module)
    case .density: String(localized: "Density", bundle: .module)
    case .recentsRetention: String(localized: "Keep history for", bundle: .module)
    case .imageCache: String(localized: "Image cache", bundle: .module)
    case .logLevel: String(localized: "What to record", bundle: .module)
    }
  }

  /// The row's subtitle: one line saying what the setting is for, in plain words.
  public var explanation: String {
    switch self {
    case .defaultEngine:
      String(
        localized: "Which player Spidola uses when a channel doesn't ask for one.",
        bundle: .module)
    case .buffering:
      String(localized: "Start faster, or ride out a patchy connection.", bundle: .module)
    case .subtitleSize:
      String(localized: "How big subtitles are drawn.", bundle: .module)
    case .subtitleBackground:
      String(localized: "What sits behind subtitles so they stay readable.", bundle: .module)
    case .language:
      String(localized: "The language Spidola is written in.", bundle: .module)
    case .density:
      String(localized: "How much fits on screen at once.", bundle: .module)
    case .recentsRetention:
      String(localized: "How long Spidola remembers what you watched.", bundle: .module)
    case .imageCache:
      String(localized: "How much space channel artwork may use.", bundle: .module)
    case .logLevel:
      String(localized: "How much detail Spidola records about what it's doing.", bundle: .module)
    }
  }

  /// The options the picker offers, in the order they are shown.
  public var choices: [SettingsChoice] {
    switch self {
    case .defaultEngine:
      DefaultEngineOption.allCases.map {
        SettingsChoice(id: $0.rawValue, label: $0.label, detail: $0.detail)
      }
    case .buffering:
      BufferingOption.allCases.map {
        SettingsChoice(id: $0.rawValue, label: $0.label, detail: $0.detail)
      }
    case .subtitleSize:
      SubtitleSizeOption.allCases.map { SettingsChoice(id: $0.rawValue, label: $0.label) }
    case .subtitleBackground:
      SubtitleBackgroundOption.allCases.map {
        SettingsChoice(id: $0.rawValue, label: $0.label, detail: $0.detail)
      }
    case .language:
      LanguageOption.allCases.map { SettingsChoice(id: $0.rawValue, label: $0.label) }
    case .density:
      DensityOption.allCases.map {
        SettingsChoice(id: $0.rawValue, label: $0.label, detail: $0.detail)
      }
    case .recentsRetention:
      RetentionOption.allCases.map { SettingsChoice(id: $0.rawValue, label: $0.label) }
    case .imageCache:
      ImageCacheOption.allCases.map { SettingsChoice(id: $0.rawValue, label: $0.label) }
    case .logLevel:
      LogLevelOption.allCases.map {
        SettingsChoice(id: $0.rawValue, label: $0.label, detail: $0.detail)
      }
    }
  }

  /// What the row's trailing accessory shows: the value in force right now.
  ///
  /// Derived from the stored number for the numeric settings rather than from a matched option, so
  /// a value none of the options offer — set on another device, or by a future build with a longer
  /// list — is reported honestly instead of being rounded to whatever we happen to show.
  public func currentValueLabel(in settings: AppSettings) -> String {
    switch self {
    case .defaultEngine: DefaultEngineOption.from(engineKey: settings.defaultEngine).label
    case .buffering: BufferingOption.from(settings.buffering).label
    case .subtitleSize: SubtitleSizeOption.from(settings.subtitleSize).label
    case .subtitleBackground: SubtitleBackgroundOption.from(settings.subtitleBackground).label
    case .language: LanguageOption.from(tag: settings.language).label
    case .density: DensityOption.from(settings.density).label
    case .recentsRetention: RetentionOption.dayCountLabel(settings.recentsRetentionDays)
    case .imageCache: ImageCacheOption.megabyteLabel(settings.imageCacheMaxMb)
    case .logLevel: LogLevelOption.from(settings.logLevel).label
    }
  }

  /// Which choice the picker marks as current, or `nil` when the stored value is none of the ones
  /// offered — in which case the picker marks nothing rather than lying about which is in force.
  public func selectedChoiceId(in settings: AppSettings) -> String? {
    switch self {
    case .defaultEngine: DefaultEngineOption.from(engineKey: settings.defaultEngine).rawValue
    case .buffering: BufferingOption.from(settings.buffering).rawValue
    case .subtitleSize: SubtitleSizeOption.from(settings.subtitleSize).rawValue
    case .subtitleBackground: SubtitleBackgroundOption.from(settings.subtitleBackground).rawValue
    case .language: LanguageOption.from(tag: settings.language).rawValue
    case .density: DensityOption.from(settings.density).rawValue
    case .recentsRetention: RetentionOption.from(days: settings.recentsRetentionDays)?.rawValue
    case .imageCache: ImageCacheOption.from(megabytes: settings.imageCacheMaxMb)?.rawValue
    case .logLevel: LogLevelOption.from(settings.logLevel).rawValue
    }
  }

  /// Writes a picked choice through to the core.
  ///
  /// An id that parses to no option leaves the setting alone: ids only ever come from `choices`,
  /// so a miss is a wiring mistake, and quietly doing nothing is the one response that cannot
  /// corrupt a stored value. `everyChoiceRoundTrips` in the tests is what actually holds the two
  /// lists together.
  func apply(choiceId: String, using access: any SettingsAccess) async throws {
    switch self {
    case .defaultEngine:
      guard let option = DefaultEngineOption(rawValue: choiceId) else { return }
      try await access.setDefaultEngine(option.engineKey)
    case .buffering:
      guard let option = BufferingOption(rawValue: choiceId) else { return }
      try await access.setBuffering(option.profile)
    case .subtitleSize:
      guard let option = SubtitleSizeOption(rawValue: choiceId) else { return }
      try await access.setSubtitleSize(option.size)
    case .subtitleBackground:
      guard let option = SubtitleBackgroundOption(rawValue: choiceId) else { return }
      try await access.setSubtitleBackground(option.background)
    case .language:
      guard let option = LanguageOption(rawValue: choiceId) else { return }
      try await access.setLanguage(option.tag)
    case .density:
      guard let option = DensityOption(rawValue: choiceId) else { return }
      try await access.setDensity(option.density)
    case .recentsRetention:
      guard let option = RetentionOption(rawValue: choiceId) else { return }
      try await access.setRecentsRetentionDays(option.days)
    case .imageCache:
      guard let option = ImageCacheOption(rawValue: choiceId) else { return }
      try await access.setImageCacheMaxMb(option.megabytes)
    case .logLevel:
      guard let option = LogLevelOption(rawValue: choiceId) else { return }
      try await access.setLogLevel(option.level)
    }
  }
}
