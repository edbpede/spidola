// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import AVFoundation
import Foundation
import PlayerContract
import XCTest

@testable import PlayerAV

/// The AVFoundation → `EngineError` classification (TECH_SPEC §8). Every case constructs its own
/// `NSError`, so the whole table is covered without a player, an asset, or a network.
///
/// XCTest rather than Swift Testing to match the Apple lane: these run in the app's test bundle
/// on the simulator (`swift test` cannot run a tvOS-triple package), the same route
/// `PlayerContractTests` and `FeatureBrowseTests` take.
final class AVErrorMappingTests: XCTestCase {

  // MARK: - Fixtures

  private func urlError(_ code: URLError.Code, wrapping underlying: NSError? = nil) -> NSError {
    error(domain: NSURLErrorDomain, code: code.rawValue, wrapping: underlying)
  }

  private func avError(_ code: AVError.Code, wrapping underlying: NSError? = nil) -> NSError {
    error(domain: AVFoundationErrorDomain, code: code.rawValue, wrapping: underlying)
  }

  private func error(domain: String, code: Int, wrapping underlying: NSError? = nil) -> NSError {
    var info: [String: Any] = [NSLocalizedDescriptionKey: "synthetic \(domain) \(code)"]
    if let underlying { info[NSUnderlyingErrorKey] = underlying }
    return NSError(domain: domain, code: code, userInfo: info)
  }

  // MARK: - NSURLErrorDomain

  func testHostAndNetworkFailuresMapToSourceUnreachable() {
    let codes: [URLError.Code] = [
      .cannotFindHost, .cannotConnectToHost, .notConnectedToInternet, .dnsLookupFailed,
    ]
    for code in codes {
      XCTAssertEqual(
        AVErrorMapping.engineError(from: urlError(code)), .sourceUnreachable,
        "URLError \(code.rawValue) should be sourceUnreachable")
    }
  }

  func testTimedOutMapsToTimeout() {
    XCTAssertEqual(AVErrorMapping.engineError(from: urlError(.timedOut)), .timeout)
  }

  func testUserAuthenticationRequiredMapsToUnauthorized() {
    XCTAssertEqual(
      AVErrorMapping.engineError(from: urlError(.userAuthenticationRequired)), .unauthorized)
  }

  /// A URL error this table does not name must not be guessed into a class — `.badURL` is not a
  /// network failure, and mapping it to one would show the viewer a retry button for a bug.
  func testUnlistedURLErrorFallsThroughToUnknown() {
    let mapped = AVErrorMapping.engineError(from: urlError(.badURL))
    XCTAssertEqual(mapped, .unknown(detail: AVErrorMapping.diagnostic(for: urlError(.badURL))))
  }

  // MARK: - AVFoundationErrorDomain

  func testContainerFailuresMapToUnsupportedFormat() {
    let codes: [AVError.Code] = [
      .failedToParse, .fileFailedToParse, .fileFormatNotRecognized, .unsupportedOutputSettings,
      .formatUnsupported,
    ]
    for code in codes {
      XCTAssertEqual(
        AVErrorMapping.engineError(from: avError(code)), .unsupportedFormat,
        "AVError \(code.rawValue) should be unsupportedFormat")
    }
  }

  func testCodecFailuresMapToDecoderFailed() {
    let codes: [AVError.Code] = [
      .decodeFailed, .decoderNotFound, .decoderTemporarilyUnavailable, .undecodableMediaData,
    ]
    for code in codes {
      XCTAssertEqual(
        AVErrorMapping.engineError(from: avError(code)), .decoderFailed,
        "AVError \(code.rawValue) should be decoderFailed")
    }
  }

  func testAuthorizationFailuresMapToUnauthorized() {
    let codes: [AVError.Code] = [.contentIsNotAuthorized, .applicationIsNotAuthorized]
    for code in codes {
      XCTAssertEqual(
        AVErrorMapping.engineError(from: avError(code)), .unauthorized,
        "AVError \(code.rawValue) should be unauthorized")
    }
  }

  /// DRM is not a rejected login. Mapping it to `.unauthorized` would tell the viewer their
  /// source "refused the login" over a FairPlay stream, which is a lie with a useless action
  /// attached.
  func testProtectedContentIsNotClassifiedAsUnauthorized() {
    XCTAssertNotEqual(AVErrorMapping.engineError(from: avError(.contentIsProtected)), .unauthorized)
  }

  /// The two classes that offer "Try other player" must not swallow codes that mean something
  /// else — a wrong confident class sends the viewer round a loop that cannot succeed.
  func testUnrelatedAVErrorsDoNotOfferAnotherPlayer() {
    let codes: [AVError.Code] = [.diskFull, .noLongerPlayable, .operationNotAllowed, .outOfMemory]
    for code in codes {
      let mapped = AVErrorMapping.engineError(from: avError(code))
      XCTAssertFalse(
        mapped.offersOtherPlayer, "AVError \(code.rawValue) should not offer another player")
    }
  }

  // MARK: - The underlying-error walk

  /// The reason this mapping walks at all: AVFoundation raises a generic `.unknown` and buries
  /// the cause that names the problem.
  func testChainIsWalkedToFindTheRealCause() {
    let buried = avError(.unknown, wrapping: urlError(.cannotConnectToHost))
    XCTAssertEqual(AVErrorMapping.engineError(from: buried), .sourceUnreachable)
  }

  func testChainIsWalkedThroughMultipleLinks() {
    let buried = avError(.unknown, wrapping: avError(.unknown, wrapping: urlError(.timedOut)))
    XCTAssertEqual(AVErrorMapping.engineError(from: buried), .timeout)
  }

  /// Outermost-first: where both levels classify, the outer error is the framework's own verdict.
  func testOutermostClassificationWinsOverABuriedOne() {
    let both = avError(.decodeFailed, wrapping: urlError(.timedOut))
    XCTAssertEqual(AVErrorMapping.engineError(from: both), .decoderFailed)
  }

  func testUnclassifiableChainBecomesUnknownWithTheWholeChainAsDetail() {
    let chain = error(domain: "CoreMediaErrorDomain", code: -12345, wrapping: avError(.unknown))
    guard case .unknown(let detail) = AVErrorMapping.engineError(from: chain) else {
      return XCTFail("expected .unknown")
    }
    XCTAssertTrue(detail.contains("CoreMediaErrorDomain -12345"), detail)
    XCTAssertTrue(
      detail.contains("\(AVFoundationErrorDomain) \(AVError.Code.unknown.rawValue)"), detail)
  }

  /// The chain arrives from a framework rather than from us, so its depth is not ours to trust.
  /// This is what stops a pathological one from spinning the walk on the playback path.
  func testChainDepthIsBounded() {
    var nested = urlError(.badURL)
    for _ in 0..<32 { nested = error(domain: "Wrap", code: 0, wrapping: nested) }
    XCTAssertLessThanOrEqual(AVErrorMapping.chain(from: nested).count, 8)
  }

  // MARK: - Diagnostics

  /// Diagnostic text goes to the log stream, never the screen (PRD §8.6) — and never carries the
  /// locator, which can embed a token in its query string (TECH_SPEC §12).
  func testDiagnosticOmitsUserInfoBeyondDomainCodeAndDescription() {
    let leaky = NSError(
      domain: NSURLErrorDomain, code: URLError.Code.badURL.rawValue,
      userInfo: [
        NSLocalizedDescriptionKey: "bad url",
        NSURLErrorFailingURLStringErrorKey: "https://portal.example/live?token=SECRET",
      ])
    let detail = AVErrorMapping.diagnostic(for: leaky)
    XCTAssertFalse(detail.contains("SECRET"), detail)
    XCTAssertTrue(detail.contains("bad url"), detail)
  }

  func testDiagnosticRendersEveryLink() {
    let chain = avError(.unknown, wrapping: urlError(.badURL))
    let detail = AVErrorMapping.diagnostic(for: chain)
    XCTAssertTrue(detail.contains(AVFoundationErrorDomain), detail)
    XCTAssertTrue(detail.contains(NSURLErrorDomain), detail)
  }

  // MARK: - HTTP status (AVPlayerItemErrorLog)

  func testAuthStatusesMapToUnauthorized() {
    XCTAssertEqual(AVErrorMapping.engineError(httpStatusCode: 401), .unauthorized)
    XCTAssertEqual(AVErrorMapping.engineError(httpStatusCode: 403), .unauthorized)
  }

  /// `0` is what the error log reports for a non-HTTP event, and every other status names no
  /// engine-level verdict — all of them must defer to the `NSError` chain rather than inventing
  /// one.
  func testNonAuthStatusesDeferToTheErrorChain() {
    for status in [0, 200, 404, 410, 500, 502, 503] {
      XCTAssertNil(
        AVErrorMapping.engineError(httpStatusCode: status),
        "HTTP \(status) should not classify on its own")
    }
  }
}
