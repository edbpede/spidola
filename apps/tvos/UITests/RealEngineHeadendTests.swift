// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import Foundation
import XCTest

/// Opt-in simulator/device acceptance against the repository's deterministic headend.
///
/// Generate the project with `SPIDOLA_HEADEND_BASE` set, then start `tools/test-headend`.
/// Each scenario launches the actual app in a debug-only acceptance mode, so the engines run in
/// the same process and with the same linked decoder stack as normal playback.
@MainActor
final class RealEngineHeadendTests: XCTestCase {
  func testAVPlayerReportsHeadendContract() async throws {
    try await assertEngine("avplayer")
  }

  func testMPVReportsHeadendContract() async throws {
    try await assertEngine("mpv")
  }

  private func assertEngine(_ engine: String) async throws {
    let base = try headendBase()
    try await assertScenario(
      engine: engine,
      locator: "\(base)/streams/hls-h264-aac/master.m3u8",
      expected: "playing")
    try await assertScenario(
      engine: engine, locator: route("unreachable", base: base, engine: engine),
      expected: "failed:SourceUnreachable")
    try await assertScenario(
      engine: engine, locator: route("unauthorized", base: base, engine: engine),
      expected: "failed:Unauthorized")
    try await assertScenario(
      engine: engine, locator: route("unsupported-format", base: base, engine: engine),
      expected: "failed:UnsupportedFormat")
    try await assertScenario(
      engine: engine, locator: route("decoder-failed", base: base, engine: engine),
      expected: "failed:DecoderFailed")
    try await assertScenario(
      engine: engine, locator: route("timeout", base: base, engine: engine),
      expected: "failed:Timeout")
    try await assertScenario(
      engine: engine, locator: route("unknown", base: base, engine: engine),
      expected: "failed:Unknown")
  }

  private func route(_ name: String, base: String, engine: String) -> String {
    if engine == "avplayer", name == "unsupported-format" {
      return "\(base)/streams/mkv-vp9-opus.mkv"
    }
    return "\(base)/\(name)\(engine == "avplayer" ? ".m3u8" : "")"
  }

  private func assertScenario(
    engine: String,
    locator: String,
    expected: String
  ) async throws {
    let app = XCUIApplication()
    app.launchEnvironment = [
      "SPIDOLA_ENGINE_ACCEPTANCE": "1",
      "SPIDOLA_ENGINE_ACCEPTANCE_ENGINE": engine,
      "SPIDOLA_ENGINE_ACCEPTANCE_LOCATOR": locator,
    ]
    app.launch()
    defer { app.terminate() }

    let result = app.staticTexts["engine-acceptance-result"]
    let deadline = ContinuousClock.now.advanced(by: .seconds(Self.routeTimeout))
    var observed = "no result"
    while ContinuousClock.now < deadline {
      if result.exists {
        observed = result.label
        if observed == expected { return }
        if observed.hasPrefix("failed:") || observed == "ended" {
          XCTFail("\(engine) reported \(observed) for \(locator); expected \(expected)")
          return
        }
      }
      try await Task.sleep(for: .milliseconds(100))
    }
    XCTFail("\(engine) remained \(observed) for \(locator); expected \(expected)")
  }

  private func headendBase() throws -> String {
    let configured =
      Bundle(for: Self.self)
      .object(forInfoDictionaryKey: "SpidolaHeadendBase") as? String
    try XCTSkipUnless(
      configured?.isEmpty == false && configured?.hasPrefix("$") == false,
      "Generate the project with SPIDOLA_HEADEND_BASE set to run real-engine acceptance")
    return requireNoTrailingSlash(try XCTUnwrap(configured))
  }

  private func requireNoTrailingSlash(_ value: String) -> String {
    value.hasSuffix("/") ? String(value.dropLast()) : value
  }

  private static let routeTimeout: TimeInterval = 75

}
