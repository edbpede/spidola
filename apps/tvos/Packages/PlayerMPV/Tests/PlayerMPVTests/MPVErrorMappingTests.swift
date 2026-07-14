// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import Libmpv
import PlayerContract
import XCTest

@testable import PlayerMPV

/// The mpv-to-contract failure mapping (TECH_SPEC §8).
///
/// This is the one part of the engine that is worth exhaustive unit testing and the one part that
/// can be: the decoder cannot run on a build machine, but the mapping is a pure function, and it
/// decides what the viewer is told and whether "Try other player" is offered at all. A wrong arm
/// here sends someone to re-enter a password over a DNS failure.
///
/// XCTest rather than Swift Testing to match the Apple lane, as `EngineSelectionTests` records:
/// these run in the app's test bundle on the simulator, because `swift test` cannot run a
/// tvOS-triple package.
final class MPVErrorMappingTests: XCTestCase {

  // MARK: - Log-line classification

  func testLogHintClassifiesUnauthorized() {
    XCTAssertEqual(MPVErrorMapping.logHint(from: "HTTP error 401 Unauthorized"), .unauthorized)
    XCTAssertEqual(MPVErrorMapping.logHint(from: "HTTP error 403 Forbidden"), .unauthorized)
    XCTAssertEqual(
      MPVErrorMapping.logHint(from: "Server returned 401 Unauthorized (authorization failed)"),
      .unauthorized)
    XCTAssertEqual(
      MPVErrorMapping.logHint(from: "Server returned 403 Forbidden"), .unauthorized)
    XCTAssertEqual(MPVErrorMapping.logHint(from: "authentication failed"), .unauthorized)
  }

  func testLogHintClassifiesSourceUnreachable() {
    XCTAssertEqual(
      MPVErrorMapping.logHint(from: "Failed to resolve hostname example.invalid"),
      .sourceUnreachable)
    XCTAssertEqual(MPVErrorMapping.logHint(from: "Connection refused"), .sourceUnreachable)
    XCTAssertEqual(MPVErrorMapping.logHint(from: "Connection reset by peer"), .sourceUnreachable)
    XCTAssertEqual(MPVErrorMapping.logHint(from: "Network is unreachable"), .sourceUnreachable)
    XCTAssertEqual(MPVErrorMapping.logHint(from: "No route to host"), .sourceUnreachable)
    XCTAssertEqual(
      MPVErrorMapping.logHint(from: "Server returned 404 Not Found"), .sourceUnreachable)
    XCTAssertEqual(
      MPVErrorMapping.logHint(from: "Server returned 502 Bad Gateway"), .sourceUnreachable)
    XCTAssertEqual(MPVErrorMapping.logHint(from: "Connection timed out"), .sourceUnreachable)
  }

  func testLogHintClassifiesUnsupportedFormat() {
    XCTAssertEqual(
      MPVErrorMapping.logHint(from: "Failed to recognize file format."), .unsupportedFormat)
    XCTAssertEqual(
      MPVErrorMapping.logHint(from: "Could not determine file format"), .unsupportedFormat)
    XCTAssertEqual(
      MPVErrorMapping.logHint(from: "Invalid data found when processing input"),
      .unsupportedFormat)
  }

  func testLogHintClassifiesDecoderFailed() {
    XCTAssertEqual(MPVErrorMapping.logHint(from: "Could not open codec."), .decoderFailed)
    XCTAssertEqual(MPVErrorMapping.logHint(from: "No decoder found for codec"), .decoderFailed)
    XCTAssertEqual(
      MPVErrorMapping.logHint(from: "Could not initialize video chain"), .decoderFailed)
    XCTAssertEqual(MPVErrorMapping.logHint(from: "Could not open video decoder"), .decoderFailed)
  }

  func testLogHintIsCaseInsensitive() {
    XCTAssertEqual(MPVErrorMapping.logHint(from: "CONNECTION REFUSED"), .sourceUnreachable)
    XCTAssertEqual(MPVErrorMapping.logHint(from: "connection refused"), .sourceUnreachable)
  }

  /// The mapping must stay silent on lines it does not recognise. A hint invented from an unrelated
  /// line would silently override the code-based mapping with a wrong class — worse than no hint,
  /// because the fallback is at least honest.
  func testLogHintReturnsNilForUnrelatedLines() {
    XCTAssertNil(MPVErrorMapping.logHint(from: "Using hardware decoding (videotoolbox)."))
    XCTAssertNil(MPVErrorMapping.logHint(from: "AO: [coreaudio] 48000Hz stereo 2ch float"))
    XCTAssertNil(MPVErrorMapping.logHint(from: ""))
  }

  /// An auth failure is also reported as a generic open failure, so the auth arms must win over the
  /// reachability arms on a line carrying both shapes. Otherwise a 401 reads as "server didn't
  /// answer" and the viewer retries forever instead of fixing their login.
  func testAuthClassificationWinsOverReachabilityOnAmbiguousLine() {
    XCTAssertEqual(
      MPVErrorMapping.logHint(from: "Failed to open https://host/x: Server returned 401"),
      .unauthorized)
  }

  // MARK: - Error-code mapping

  /// The disambiguation the taxonomy depends on: one mpv code, four viewer-visible outcomes,
  /// separated only by what the log said.
  func testLoadingFailedIsDisambiguatedByLogHint() {
    XCTAssertEqual(
      MPVErrorMapping.engineError(mpvError: MPV_ERROR_LOADING_FAILED.rawValue, logHint: nil),
      .sourceUnreachable)
    XCTAssertEqual(
      MPVErrorMapping.engineError(
        mpvError: MPV_ERROR_LOADING_FAILED.rawValue, logHint: .unauthorized),
      .unauthorized)
    XCTAssertEqual(
      MPVErrorMapping.engineError(
        mpvError: MPV_ERROR_LOADING_FAILED.rawValue, logHint: .unsupportedFormat),
      .unsupportedFormat)
    XCTAssertEqual(
      MPVErrorMapping.engineError(
        mpvError: MPV_ERROR_LOADING_FAILED.rawValue, logHint: .decoderFailed),
      .decoderFailed)
  }

  func testFormatCodesMapToUnsupportedFormat() {
    XCTAssertEqual(
      MPVErrorMapping.engineError(mpvError: MPV_ERROR_UNKNOWN_FORMAT.rawValue),
      .unsupportedFormat)
    XCTAssertEqual(
      MPVErrorMapping.engineError(mpvError: MPV_ERROR_NOTHING_TO_PLAY.rawValue),
      .unsupportedFormat)
    XCTAssertEqual(
      MPVErrorMapping.engineError(mpvError: MPV_ERROR_UNSUPPORTED.rawValue), .unsupportedFormat)
  }

  func testOutputInitCodesMapToDecoderFailed() {
    XCTAssertEqual(
      MPVErrorMapping.engineError(mpvError: MPV_ERROR_AO_INIT_FAILED.rawValue), .decoderFailed)
    XCTAssertEqual(
      MPVErrorMapping.engineError(mpvError: MPV_ERROR_VO_INIT_FAILED.rawValue), .decoderFailed)
    XCTAssertEqual(MPVErrorMapping.engineError(mpvError: MPV_ERROR_NOMEM.rawValue), .decoderFailed)
  }

  func testUnclassifiedCodeMapsToUnknownWithDetail() {
    let error = MPVErrorMapping.engineError(mpvError: MPV_ERROR_GENERIC.rawValue)
    guard case .unknown(let detail) = error else {
      return XCTFail("expected .unknown, got \(error)")
    }
    XCTAssertFalse(detail.isEmpty)
  }

  func testSuccessOnAFailurePathMapsToUnknown() {
    let error = MPVErrorMapping.engineError(mpvError: MPV_ERROR_SUCCESS.rawValue)
    guard case .unknown = error else { return XCTFail("expected .unknown, got \(error)") }
  }

  /// A hint must not leak into codes that are already unambiguous — otherwise the mapping depends on
  /// whichever log line happened to arrive last, which is the non-determinism the design forbids.
  func testLogHintIsIgnoredForUnambiguousCodes() {
    XCTAssertEqual(
      MPVErrorMapping.engineError(
        mpvError: MPV_ERROR_UNKNOWN_FORMAT.rawValue, logHint: .unauthorized),
      .unsupportedFormat)
    XCTAssertEqual(
      MPVErrorMapping.engineError(
        mpvError: MPV_ERROR_AO_INIT_FAILED.rawValue, logHint: .sourceUnreachable),
      .decoderFailed)
  }

  // MARK: - End-of-file mapping

  func testEndFileEOFIsEnded() {
    XCTAssertEqual(
      MPVErrorMapping.endFileOutcome(reason: MPV_END_FILE_REASON_EOF, mpvError: 0), .ended)
  }

  func testEndFileErrorIsFailedWithMappedError() {
    XCTAssertEqual(
      MPVErrorMapping.endFileOutcome(
        reason: MPV_END_FILE_REASON_ERROR, mpvError: MPV_ERROR_UNKNOWN_FORMAT.rawValue),
      .failed(.unsupportedFormat))
    XCTAssertEqual(
      MPVErrorMapping.endFileOutcome(
        reason: MPV_END_FILE_REASON_ERROR, mpvError: MPV_ERROR_LOADING_FAILED.rawValue,
        logHint: .unauthorized),
      .failed(.unauthorized))
  }

  /// Our own teardown must be silent. Reporting these would race a spurious `.failed` onto the
  /// stream on every channel change, since a zap stops the engine by destroying its core.
  func testEndFileReasonsWeCausedAreSilent() {
    XCTAssertNil(
      MPVErrorMapping.endFileOutcome(reason: MPV_END_FILE_REASON_STOP, mpvError: 0))
    XCTAssertNil(
      MPVErrorMapping.endFileOutcome(reason: MPV_END_FILE_REASON_QUIT, mpvError: 0))
  }

  /// mpv is about to load the real target; the state machine should stay in `.loading` across it.
  func testEndFileRedirectIsSilent() {
    XCTAssertNil(
      MPVErrorMapping.endFileOutcome(reason: MPV_END_FILE_REASON_REDIRECT, mpvError: 0))
  }

  // MARK: - Diagnostics

  /// `unknown(detail:)` reaches the log stream, so its content must come from mpv's static string
  /// table and never from stream data (TECH_SPEC §12).
  func testDescribeUsesMPVStaticStringTable() {
    let described = MPVErrorMapping.describe(mpvError: MPV_ERROR_LOADING_FAILED.rawValue)
    XCTAssertTrue(described.contains("-13"))
    XCTAssertTrue(described.lowercased().contains("loading failed"))
  }
}
