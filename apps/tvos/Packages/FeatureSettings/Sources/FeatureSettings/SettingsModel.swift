// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import CoreKit
import Observation
import core_api

/// One row on the settings root. Most settings are a closed set behind a picker; the three that
/// are not each behave differently enough to be their own case rather than a flag on `choice`.
public enum SettingsRow: Hashable, Sendable {
  /// Opens a picker for a closed-set setting.
  case choice(SettingsField)
  /// Turns recently-watched recording on or off in place — no picker for two states.
  case recentsSwitch
  /// Deletes the history now. Destructive, so it says so and sits last in its section.
  case clearRecents
  /// Opens the diagnostics screen.
  case diagnostics
}

/// A titled group of rows on the settings root.
public struct SettingsSection: Identifiable, Sendable {
  public let id: String
  public let title: String
  public let rows: [SettingsRow]
}

/// Backs the settings root: one vertical, D-pad-traversable list of rows under section headers,
/// each showing its setting's name, what it is for, and the value in force (PRD §6.9). Reads the
/// whole surface in one `settingsSnapshot()` so every row's value comes from the same instant, and
/// re-reads after each change so the screen always shows the core rather than what it hoped it
/// wrote.
///
/// Every failure — load or write — lands in `.failed` with a fully-formed `ActionableError`, so
/// the screen can never present a dead end (PRD §6.3). There is no bare status string here on
/// purpose: a settings write that failed leaves the screen showing a value that is not in force,
/// and "Try again" is the only honest thing to offer.
@MainActor
@Observable
public final class SettingsModel {
  public private(set) var state: LoadState<AppSettings> = .loading

  private let access: any SettingsAccess

  public init(access: any SettingsAccess) {
    self.access = access
  }

  /// The information architecture, in order. Pure and static: it is the same list every launch,
  /// and keeping it out of the view means a test can assert the shape of the screen without
  /// rendering one.
  ///
  /// The log level is not here — it belongs to the diagnostics screen, which is where someone
  /// looking to change it already is.
  public static let sections: [SettingsSection] = [
    SettingsSection(
      id: "playback",
      title: String(localized: "Playback", bundle: .module),
      rows: [.choice(.defaultEngine), .choice(.buffering)]),
    SettingsSection(
      id: "subtitles",
      title: String(localized: "Subtitles", bundle: .module),
      rows: [.choice(.subtitleSize), .choice(.subtitleBackground)]),
    SettingsSection(
      id: "interface",
      title: String(localized: "Interface", bundle: .module),
      rows: [.choice(.language), .choice(.density)]),
    SettingsSection(
      id: "privacy",
      title: String(localized: "Privacy", bundle: .module),
      rows: [.recentsSwitch, .choice(.recentsRetention), .clearRecents]),
    SettingsSection(
      id: "storage",
      title: String(localized: "Storage", bundle: .module),
      rows: [.choice(.imageCache)]),
    SettingsSection(
      id: "diagnostics",
      title: String(localized: "Diagnostics", bundle: .module),
      rows: [.diagnostics]),
  ]

  public func load() async {
    // A reload behind a visible screen keeps the current values on screen rather than flashing a
    // spinner over rows that are about to look almost identical.
    if case .ready = state {} else { state = .loading }
    do {
      state = .ready(try await access.settingsSnapshot())
    } catch {
      if let failed = LoadState<AppSettings>.failure(from: error) { state = failed }
    }
  }

  /// Writes a picked choice for `field`. The pickers call this; the root only reloads after.
  public func apply(_ field: SettingsField, choiceId: String) async {
    await mutate { try await field.apply(choiceId: choiceId, using: self.access) }
  }

  /// Turns recently-watched recording on or off.
  ///
  /// Routes to the **recents** API, not a settings setter: `settingsSnapshot()` reports this
  /// switch, but the core's `RecentsService` is its only writer because that service is what
  /// enforces it. Reading it from one place and writing it to another looks like a mistake and is
  /// not one.
  public func setRecentsEnabled(_ enabled: Bool) async {
    await mutate { try await self.access.setRecentsEnabled(enabled) }
  }

  /// Deletes recently-watched history now.
  public func clearRecents() async {
    await mutate { try await self.access.clearRecents() }
  }

  /// Runs a write, then re-reads the snapshot so the rows show what the core actually holds.
  private func mutate(_ action: @escaping () async throws -> Void) async {
    do {
      try await action()
      await load()
    } catch {
      if let failed = LoadState<AppSettings>.failure(from: error) { state = failed }
    }
  }
}
