// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import CoreKit
import Foundation
import XCTest
import core_api

@testable import FeatureSettings

@MainActor
final class SettingsModelTests: XCTestCase {
  // MARK: - Root

  func testSnapshotLoadsIntoRows() async {
    let access = FakeSettingsAccess()
    access.settings.defaultEngine = "avplayer"
    access.settings.recentsRetentionDays = 365
    let model = SettingsModel(access: access)
    await model.load()

    guard case .ready(let settings) = model.state else { return XCTFail("expected ready") }
    XCTAssertEqual(SettingsField.defaultEngine.currentValueLabel(in: settings), "AVPlayer")
    XCTAssertEqual(SettingsField.recentsRetention.currentValueLabel(in: settings), "365 days")
    // The value in force is what the row shows, for every closed-set row on the screen.
    let fields = SettingsModel.sections.flatMap(\.rows).compactMap { row -> SettingsField? in
      if case .choice(let field) = row { return field }
      return nil
    }
    XCTAssertFalse(fields.isEmpty)
    for field in fields {
      XCTAssertFalse(
        field.currentValueLabel(in: settings).isEmpty, "\(field) had no value to show")
    }
  }

  /// The one test holding `choices`, `apply`, and `selectedChoiceId` together. Each is a separate
  /// `switch` over the same nine fields, so nothing but this stops a new setting from offering an
  /// option that writes a different setting's value — or writes nothing at all.
  func testEveryChoiceRoundTrips() async {
    for field in SettingsField.allCases {
      for choice in field.choices {
        let access = FakeSettingsAccess()
        let model = SettingsModel(access: access)
        await model.load()
        await model.apply(field, choiceId: choice.id)

        guard case .ready(let settings) = model.state else {
          return XCTFail("expected ready after \(field)/\(choice.id)")
        }
        XCTAssertEqual(
          field.selectedChoiceId(in: settings), choice.id,
          "\(field) did not round-trip choice \(choice.id)")
      }
    }
  }

  func testDefaultEngineChoicesMapToTheContractsEngineKeys() async {
    let access = FakeSettingsAccess()
    let model = SettingsModel(access: access)
    await model.load()

    await model.apply(.defaultEngine, choiceId: DefaultEngineOption.mpv.rawValue)
    XCTAssertEqual(access.settings.defaultEngine, "mpv")

    await model.apply(.defaultEngine, choiceId: DefaultEngineOption.avPlayer.rawValue)
    XCTAssertEqual(access.settings.defaultEngine, "avplayer")

    // Automatic is the *absence* of an override, not a third engine key: the core must store nil
    // so the selection policy falls through to the platform default.
    await model.apply(.defaultEngine, choiceId: DefaultEngineOption.automatic.rawValue)
    XCTAssertNil(access.settings.defaultEngine)
  }

  /// An engine key this build cannot construct must still render as a row, not a blank.
  func testUnknownStoredEngineReadsAsAutomatic() {
    var settings = AppSettings.fake()
    settings.defaultEngine = "some-engine-from-a-newer-build"
    XCTAssertEqual(SettingsField.defaultEngine.currentValueLabel(in: settings), "Automatic")
  }

  /// A stored value none of the offered options matches is reported honestly and marked nowhere,
  /// rather than being rounded to whichever option happens to be nearest.
  func testStoredValueOutsideTheOfferedSetIsShownButNotMarked() {
    var settings = AppSettings.fake()
    settings.recentsRetentionDays = 45
    XCTAssertEqual(SettingsField.recentsRetention.currentValueLabel(in: settings), "45 days")
    XCTAssertNil(SettingsField.recentsRetention.selectedChoiceId(in: settings))
  }

  func testFailingAccessSurfacesAnActionableError() async {
    let access = FakeSettingsAccess()
    access.failure = .StorageCorrupt
    let model = SettingsModel(access: access)
    await model.load()

    guard case .failed(let error) = model.state else { return XCTFail("expected failed") }
    // PRD §6.3: an error with no action is a design bug, so the type makes one unrepresentable.
    XCTAssertFalse(error.actions.isEmpty)
    XCTAssertFalse(error.message.isEmpty)
  }

  /// A write that fails must not leave the rows showing a value that is not in force.
  func testFailingWriteSurfacesAnActionableError() async {
    let access = FakeSettingsAccess()
    let model = SettingsModel(access: access)
    await model.load()
    access.failure = .StorageCorrupt
    await model.apply(.density, choiceId: DensityOption.compact.rawValue)

    guard case .failed(let error) = model.state else { return XCTFail("expected failed") }
    XCTAssertFalse(error.actions.isEmpty)
  }

  // MARK: - Privacy

  /// The switch is *reported* by the settings snapshot but *owned* by the recents service, so the
  /// toggle has to write through the recents API. Reading from one place and writing to another
  /// looks like a bug, which is exactly why it needs a test saying it is not.
  func testRecentsSwitchRoutesToTheRecentsApiNotTheSettingsOne() async {
    let access = FakeSettingsAccess()
    let model = SettingsModel(access: access)
    await model.load()

    await model.setRecentsEnabled(false)

    XCTAssertEqual(access.recentsEnabledWrites, [false])
    XCTAssertTrue(
      access.settingsWrites.isEmpty,
      "the off-switch must not be written through the settings vocabulary")
    guard case .ready(let settings) = model.state else { return XCTFail("expected ready") }
    XCTAssertFalse(settings.recentsEnabled)
  }

  func testClearRecentsRoutesToTheRecentsApi() async {
    let access = FakeSettingsAccess()
    let model = SettingsModel(access: access)
    await model.load()

    await model.clearRecents()

    XCTAssertEqual(access.clearRecentsCalls, 1)
    XCTAssertTrue(access.settingsWrites.isEmpty)
  }

  // MARK: - Information architecture

  /// The EPG window is in the core's vocabulary because PRD §6.9 lists it, but EPG ingest is
  /// Phase 8 — a row that changed a number nobody could observe would be a UX bug. This is the
  /// guard that stops it being added back by reflex before the screens that give it meaning.
  func testEpgWindowIsNotOffered() {
    XCTAssertFalse(SettingsField.allCases.contains { $0.rawValue.lowercased().contains("epg") })
  }

  /// The log level belongs to the diagnostics screen, not the root — someone who wants to change
  /// it is already there.
  func testRootDoesNotOfferTheLogLevel() {
    XCTAssertFalse(SettingsModel.sections.flatMap(\.rows).contains(.choice(.logLevel)))
  }

  func testEveryRootRowIsReachableAndNoFieldIsOfferedTwice() {
    let fields = SettingsModel.sections.flatMap(\.rows).compactMap { row -> SettingsField? in
      if case .choice(let field) = row { return field }
      return nil
    }
    XCTAssertEqual(Set(fields).count, fields.count, "a setting is offered on two rows")
    XCTAssertTrue(SettingsModel.sections.flatMap(\.rows).contains(.diagnostics))
    XCTAssertTrue(SettingsModel.sections.flatMap(\.rows).contains(.about))
    XCTAssertTrue(SettingsModel.sections.flatMap(\.rows).contains(.recentsSwitch))
    XCTAssertTrue(SettingsModel.sections.flatMap(\.rows).contains(.clearRecents))
  }

  // MARK: - Picker

  func testPickerMarksTheCurrentValue() async {
    let access = FakeSettingsAccess()
    access.settings.subtitleSize = .large
    let model = SettingsOptionsModel(field: .subtitleSize, access: access)
    await model.load()

    XCTAssertEqual(model.selectedChoiceId, SubtitleSizeOption.large.rawValue)
    XCTAssertEqual(model.choices.map(\.id), SubtitleSizeOption.allCases.map(\.rawValue))
  }

  func testPickerChoosingWritesAndReportsSuccess() async {
    let access = FakeSettingsAccess()
    let model = SettingsOptionsModel(field: .subtitleBackground, access: access)
    await model.load()

    let applied = await model.choose(SubtitleBackgroundOption.solid.rawValue)

    XCTAssertTrue(applied)
    XCTAssertEqual(access.settings.subtitleBackground, .solid)
  }

  /// A failed write must not close the picker: popping back to a root still showing the old value
  /// would tell the viewer their change was saved when it was not.
  func testPickerKeepsTheScreenOpenWhenTheWriteFails() async {
    let access = FakeSettingsAccess()
    let model = SettingsOptionsModel(field: .subtitleBackground, access: access)
    await model.load()
    access.failure = .StorageCorrupt

    let applied = await model.choose(SubtitleBackgroundOption.solid.rawValue)

    XCTAssertFalse(applied)
    guard case .failed(let error) = model.state else { return XCTFail("expected failed") }
    XCTAssertFalse(error.actions.isEmpty)
  }

  // MARK: - Diagnostics

  func testDiagnosticsReportsActivityLevelAndVersions() async {
    let access = FakeSettingsAccess(
      logLines: ["one", "two"],
      handshake: Handshake(
        coreVersion: "0.4.2", coreGitRevision: "abc1234", schemaVersion: 1, boundaryVersion: 2))
    access.settings.logLevel = .debug
    let model = DiagnosticsModel(access: access, infoValue: Self.fakeInfo)
    await model.load()

    guard case .ready(let content) = model.state else { return XCTFail("expected ready") }
    XCTAssertEqual(content.recentActivity, ["one", "two"])
    XCTAssertEqual(content.logLevel, .debug)
    XCTAssertEqual(
      content.versions.map(\.value), ["1.2.3 (45)", "0.4.2", "abc1234", "1", "2"])
  }

  /// A diagnostics screen that trapped on a missing bundle key would take the app down exactly
  /// when someone was trying to report what was wrong with it.
  func testDiagnosticsReportsAnUnknownAppVersionRatherThanTrapping() async {
    let access = FakeSettingsAccess()
    let model = DiagnosticsModel(access: access, infoValue: { _ in nil })
    await model.load()

    guard case .ready(let content) = model.state else { return XCTFail("expected ready") }
    XCTAssertEqual(content.versions.first?.id, "app")
    XCTAssertEqual(content.versions.first?.value, "Unknown")
  }

  /// A build with a marketing version but no build number still reports something true.
  func testDiagnosticsReportsAMarketingVersionWithoutABuild() async {
    let access = FakeSettingsAccess()
    let model = DiagnosticsModel(
      access: access,
      infoValue: { $0 == "CFBundleShortVersionString" ? "1.2.3" : nil })
    await model.load()

    guard case .ready(let content) = model.state else { return XCTFail("expected ready") }
    XCTAssertEqual(content.versions.first?.value, "1.2.3")
  }

  func testDiagnosticsFailureSurfacesAnActionableError() async {
    let access = FakeSettingsAccess()
    access.failure = .Internal
    let model = DiagnosticsModel(access: access)
    await model.load()

    guard case .failed(let error) = model.state else { return XCTFail("expected failed") }
    XCTAssertFalse(error.actions.isEmpty)
  }

  func testLogLevelRoundTripsThroughTheDiagnosticsPicker() async {
    let access = FakeSettingsAccess()
    let model = SettingsOptionsModel(field: .logLevel, access: access)
    await model.load()

    let applied = await model.choose(LogLevelOption.trace.rawValue)

    XCTAssertTrue(applied)
    XCTAssertEqual(access.settings.logLevel, .trace)
  }
}

// MARK: - Fakes

extension AppSettings {
  /// The core's own defaults, spelled out here rather than read from the core: a test that asked
  /// the core what the defaults were could not notice the core changing them.
  static func fake() -> AppSettings {
    AppSettings(
      defaultEngine: nil,
      buffering: .balanced,
      subtitleSize: .medium,
      subtitleBackground: .shadow,
      language: nil,
      density: .comfortable,
      recentsEnabled: true,
      recentsRetentionDays: 90,
      epgWindowAheadHours: 72,
      epgWindowBehindHours: 6,
      imageCacheMaxMb: 256,
      logLevel: .info)
  }
}

extension SettingsModelTests {
  /// Stands in for the app bundle's version keys.
  fileprivate static func fakeInfo(_ key: String) -> String? {
    switch key {
    case "CFBundleShortVersionString": "1.2.3"
    case "CFBundleVersion": "45"
    default: nil
    }
  }
}

/// A fake `SettingsAccess` that records which API each write went through and mutates its snapshot,
/// so a setter can be asserted to actually round-trip rather than merely to have been called.
///
/// `@MainActor`-isolated rather than `@unchecked Sendable`: the isolation is what makes it
/// `Sendable`, so the compiler still proves the absence of races instead of being told to trust
/// this. The two synchronous members of the protocol are `nonisolated` and read immutable `let`s,
/// which is why that works.
@MainActor
private final class FakeSettingsAccess: SettingsAccess {
  var settings: AppSettings = .fake()
  var failure: ApiError?

  /// Every write that went through the *settings* vocabulary, by setting name.
  private(set) var settingsWrites: [String] = []
  /// Every write that went through the *recents* off-switch.
  private(set) var recentsEnabledWrites: [Bool] = []
  private(set) var clearRecentsCalls = 0

  nonisolated let logLines: [String]
  nonisolated let handshakeValue: Handshake

  init(
    logLines: [String] = [],
    handshake: Handshake = Handshake(
      coreVersion: "0.0.0", coreGitRevision: "0000000", schemaVersion: 1, boundaryVersion: 2)
  ) {
    self.logLines = logLines
    self.handshakeValue = handshake
  }

  private func check(_ name: String) throws {
    if let failure { throw failure }
    settingsWrites.append(name)
  }

  func settingsSnapshot() async throws -> AppSettings {
    if let failure { throw failure }
    return settings
  }

  func setDefaultEngine(_ engine: String?) async throws {
    try check("defaultEngine")
    settings.defaultEngine = engine
  }

  func setBuffering(_ profile: BufferingProfile) async throws {
    try check("buffering")
    settings.buffering = profile
  }

  func setSubtitleSize(_ size: SubtitleSize) async throws {
    try check("subtitleSize")
    settings.subtitleSize = size
  }

  func setSubtitleBackground(_ background: SubtitleBackground) async throws {
    try check("subtitleBackground")
    settings.subtitleBackground = background
  }

  func setLanguage(_ tag: String?) async throws {
    try check("language")
    settings.language = tag
  }

  func setDensity(_ density: InterfaceDensity) async throws {
    try check("density")
    settings.density = density
  }

  func setRecentsRetentionDays(_ days: UInt32) async throws {
    try check("recentsRetentionDays")
    settings.recentsRetentionDays = days
  }

  func setImageCacheMaxMb(_ megabytes: UInt32) async throws {
    try check("imageCacheMaxMb")
    settings.imageCacheMaxMb = megabytes
  }

  func setLogLevel(_ level: LogLevel) async throws {
    try check("logLevel")
    settings.logLevel = level
  }

  func recentsEnabled() async throws -> Bool {
    if let failure { throw failure }
    return settings.recentsEnabled
  }

  func setRecentsEnabled(_ enabled: Bool) async throws {
    if let failure { throw failure }
    recentsEnabledWrites.append(enabled)
    settings.recentsEnabled = enabled
  }

  func clearRecents() async throws {
    if let failure { throw failure }
    clearRecentsCalls += 1
  }

  nonisolated func exportLogs() -> [String] { logLines }
  nonisolated func handshake() -> Handshake { handshakeValue }
}
