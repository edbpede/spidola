// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import CoreKit
import Foundation
import Observation
import core_api

/// One version fact, as a label and a value.
public struct VersionFact: Identifiable, Hashable, Sendable {
  public let id: String
  public let label: String
  public let value: String
}

/// Everything the diagnostics screen shows, read in one pass so the level, the activity, and the
/// versions all describe the same moment.
public struct DiagnosticsContent: Sendable {
  public let logLevel: LogLevel
  /// The core's log ring, oldest first.
  public let recentActivity: [String]
  public let versions: [VersionFact]
}

/// Backs the diagnostics screen (PRD §6.9): what Spidola records, what it has recorded lately, and
/// which versions are running.
///
/// "Export logs" in the PRD is **a viewer here, not a file**. tvOS shows the user no file system,
/// mounts no volumes, and offers nowhere to put a text file they could later retrieve — a written
/// log would be unreachable by the person who needs it. Putting the lines on screen, where they can
/// be read out or photographed, is the only form of export the platform actually supports.
@MainActor
@Observable
public final class DiagnosticsModel {
  public private(set) var state: LoadState<DiagnosticsContent> = .loading

  private let access: any SettingsAccess
  private let infoValue: (String) -> String?

  /// - Parameter bundle: where the app's own version is read from. The app is the only caller;
  ///   `.main` is the bundle the marketing version and build live in.
  public convenience init(access: any SettingsAccess, bundle: Bundle = .main) {
    self.init(access: access) { bundle.object(forInfoDictionaryKey: $0) as? String }
  }

  /// The seam a test uses to assert the versions block without an app around it.
  ///
  /// A closure rather than a `Bundle` subclass because `Bundle` is a class cluster with no
  /// public `init()` to subclass through, and pushing a fake one in would mean an
  /// `@unchecked Sendable` escape hatch on a type that exists only to answer two keys.
  init(access: any SettingsAccess, infoValue: @escaping (String) -> String?) {
    self.access = access
    self.infoValue = infoValue
  }

  public func load() async {
    if case .ready = state {} else { state = .loading }
    do {
      let settings = try await access.settingsSnapshot()
      state = .ready(
        DiagnosticsContent(
          logLevel: settings.logLevel,
          recentActivity: access.exportLogs(),
          versions: versionFacts()))
    } catch {
      if let failed = LoadState<DiagnosticsContent>.failure(from: error) { state = failed }
    }
  }

  /// The versions block. The labels are the plainest true names for each number rather than the
  /// internal ones ("Data format", not "schema version"): the reader is a household member being
  /// asked what their app says, and they have to read these aloud.
  private func versionFacts() -> [VersionFact] {
    let handshake = access.handshake()
    return [
      VersionFact(
        id: "app", label: String(localized: "App", bundle: .module), value: appVersion()),
      VersionFact(
        id: "core", label: String(localized: "Core", bundle: .module),
        value: handshake.coreVersion),
      VersionFact(
        id: "revision", label: String(localized: "Core revision", bundle: .module),
        value: handshake.coreGitRevision),
      VersionFact(
        id: "schema", label: String(localized: "Data format", bundle: .module),
        value: String(handshake.schemaVersion)),
      VersionFact(
        id: "boundary", label: String(localized: "Bridge", bundle: .module),
        value: String(handshake.boundaryVersion)),
    ]
  }

  /// The app's marketing version and build, or a plain "Unknown" when the bundle carries neither —
  /// a diagnostics screen that trapped on a missing key would take the app down exactly when
  /// someone was trying to report what was wrong with it.
  private func appVersion() -> String {
    let short = infoValue("CFBundleShortVersionString")
    let build = infoValue("CFBundleVersion")
    switch (short, build) {
    case (let short?, let build?): return "\(short) (\(build))"
    case (let short?, nil): return short
    case (nil, let build?): return build
    case (nil, nil): return String(localized: "Unknown", bundle: .module)
    }
  }
}
