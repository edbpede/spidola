// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import PlayerContract
import XCTest

/// The selection policy (TECH_SPEC §8) and the loud-fallback rule. Pure logic — the Kotlin mirror
/// (`EngineSelectionTest`) asserts the same cases, which is what keeps "identical on both
/// platforms" from being a slogan.
///
/// XCTest rather than Swift Testing to match the Apple lane: these run in the app's test bundle on
/// the simulator (`swift test` cannot run a tvOS-triple package), the same route
/// `FeatureBrowseTests` already takes.
final class EngineSelectionTests: XCTestCase {
  private let mpv = EngineID.mpv
  private let av = EngineID.avPlayer
  private var both: Set<EngineID> { [mpv, av] }

  func testEngineRequestDiagnosticsRedactCredentials() {
    let secret = "credential-value"
    let request = StreamRequest(
      locator: "https://stream.example/\(secret)",
      headers: [StreamHeader(name: "Authorization", value: secret)],
      userAgent: "Bearer-\(secret)")

    XCTAssertFalse(String(reflecting: request).contains(secret))
  }

  // MARK: - Precedence: channel → source → platform default

  func testChannelOverrideWinsOverSourceAndDefault() {
    let resolved = EngineSelection.resolve(
      channelOverride: av, sourceOverride: mpv, platformDefault: mpv, registered: both)
    XCTAssertEqual(resolved, av)
  }

  func testSourceOverrideWinsWhenNoChannelOverride() {
    let resolved = EngineSelection.resolve(
      channelOverride: nil, sourceOverride: av, platformDefault: mpv, registered: both)
    XCTAssertEqual(resolved, av)
  }

  func testPlatformDefaultWhenNoOverrides() {
    let resolved = EngineSelection.resolve(
      channelOverride: nil, sourceOverride: nil, platformDefault: mpv, registered: both)
    XCTAssertEqual(resolved, mpv)
  }

  // MARK: - Stale / foreign override keys

  /// Overrides are opaque strings that outlive builds: a key naming an engine this build does not
  /// link must never make a channel unplayable.
  func testUnregisteredChannelOverrideFallsThroughToSource() {
    let resolved = EngineSelection.resolve(
      channelOverride: EngineID(rawValue: "exoplayer"),
      sourceOverride: av, platformDefault: mpv, registered: both)
    XCTAssertEqual(resolved, av)
  }

  func testUnregisteredOverridesFallThroughToDefault() {
    let resolved = EngineSelection.resolve(
      channelOverride: EngineID(rawValue: "future-engine"),
      sourceOverride: EngineID(rawValue: "exoplayer"),
      platformDefault: mpv, registered: both)
    XCTAssertEqual(resolved, mpv)
  }

  /// The default is returned even when unregistered, so the caller reports one honest failure
  /// rather than the policy inventing a substitute.
  func testDefaultReturnedEvenWhenNotRegistered() {
    let resolved = EngineSelection.resolve(
      channelOverride: nil, sourceOverride: nil, platformDefault: mpv, registered: [])
    XCTAssertEqual(resolved, mpv)
  }

  // MARK: - "Try other player" target

  func testAlternateIsTheOtherRegisteredEngine() {
    XCTAssertEqual(EngineSelection.alternate(to: mpv, registered: both), av)
    XCTAssertEqual(EngineSelection.alternate(to: av, registered: both), mpv)
  }

  func testAlternateIsNilWhenNothingElseIsRegistered() {
    XCTAssertNil(EngineSelection.alternate(to: mpv, registered: [mpv]))
  }

  /// A non-deterministic offer would make "remember for this channel" remember a choice the
  /// viewer did not make.
  func testAlternateIsDeterministic() {
    let registered: Set<EngineID> = [mpv, av, EngineID(rawValue: "zzz")]
    let first = EngineSelection.alternate(to: mpv, registered: registered)
    for _ in 0..<32 {
      XCTAssertEqual(EngineSelection.alternate(to: mpv, registered: registered), first)
    }
  }

  // MARK: - Loud fallback

  /// Only a format/decode failure means another engine could plausibly succeed (TECH_SPEC §8).
  func testOnlyFormatAndDecodeFailuresOfferAnotherPlayer() {
    XCTAssertTrue(EngineError.unsupportedFormat.offersOtherPlayer)
    XCTAssertTrue(EngineError.decoderFailed.offersOtherPlayer)
    XCTAssertFalse(EngineError.sourceUnreachable.offersOtherPlayer)
    XCTAssertFalse(EngineError.unauthorized.offersOtherPlayer)
    XCTAssertFalse(EngineError.timeout.offersOtherPlayer)
    XCTAssertFalse(EngineError.unknown(detail: "boom").offersOtherPlayer)
  }

  /// Every variant, exhaustively: adding one forces a UX decision here rather than shipping a
  /// blank screen (PRD §6.3 — an error with no action is a design bug).
  func testEveryErrorVariantHasCouchLegibleCopy() {
    let all: [EngineError] = [
      .sourceUnreachable, .unauthorized, .unsupportedFormat, .decoderFailed, .timeout,
      .unknown(detail: "detail"),
    ]
    for error in all {
      XCTAssertFalse(error.failureClass.isEmpty, "\(error) has no failure class")
      XCTAssertFalse(error.message.isEmpty, "\(error) has no message")
    }
  }

  /// Diagnostic chains go to the log stream, never the screen (PRD §8.6).
  func testOnlyUnknownCarriesDiagnosticDetail() {
    XCTAssertEqual(EngineError.unknown(detail: "mpv: -10").diagnosticDetail, "mpv: -10")
    XCTAssertNil(EngineError.decoderFailed.diagnosticDetail)
  }
}
