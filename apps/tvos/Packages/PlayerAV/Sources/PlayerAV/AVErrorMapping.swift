// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import AVFoundation
import CoreMedia
import Foundation
import PlayerContract

/// Classifies AVFoundation's native failures into the shared `EngineError` taxonomy
/// (TECH_SPEC §8).
///
/// Pure by construction: every entry point takes a value and returns a verdict, touching no
/// player, asset, or network. That is deliberate — this classification decides whether a viewer
/// is offered "Try other player" or told the channel is unreachable (PRD §6.3), and a decision
/// that visible should be testable against a constructed error rather than only against a live
/// decode that misbehaves on someone else's hardware.
public enum AVErrorMapping {
  /// Classifies `error`, walking its underlying-error chain outermost-first.
  ///
  /// The walk is the substance of this function. AVFoundation reports a generic
  /// `AVError.unknown` at the top of most playback failures and buries the cause that actually
  /// names the problem — an `NSURLError`, or a lower `AVError` — one or two links down
  /// `NSUnderlyingErrorKey`. Classifying only the outermost error would collapse the majority of
  /// real failures into `.unknown` and cost the viewer the actionable message PRD §6.3 requires.
  ///
  /// Outermost-first because the outer error is the one AVFoundation chose to raise: where both
  /// levels classify, the outer one is the framework's own verdict about what went wrong.
  public static func engineError(from error: NSError) -> EngineError {
    for link in chain(from: error) {
      if let verdict = classify(link) { return verdict }
    }
    return .unknown(detail: diagnostic(for: error))
  }

  /// Classifies an HTTP status read off `AVPlayerItemErrorLog`.
  ///
  /// Separate from the `NSError` path because AVFoundation does not put the status in the error
  /// at all: an HLS variant that is refused surfaces as an undocumented `CoreMediaErrorDomain`
  /// code, and Apple publishes no mapping from those codes to HTTP statuses. The error log's
  /// events do carry the status, so this engine reads it where it is actually published instead
  /// of pattern-matching numbers that only exist in folklore and could change without notice.
  ///
  /// Returns `nil` for every status that names no engine-level verdict — including the `0` the
  /// log reports for non-HTTP events — leaving the `NSError` chain to classify.
  public static func engineError(httpStatusCode: Int) -> EngineError? {
    switch httpStatusCode {
    case 401, 403: .unauthorized
    default: nil
    }
  }

  /// The error and its `NSUnderlyingErrorKey` ancestry, outermost first.
  static func chain(from error: NSError) -> [NSError] {
    var links: [NSError] = []
    var current: NSError? = error
    // A malformed chain that points back at itself would spin here forever. A real chain is two
    // or three links, so a depth bound costs nothing and removes the possibility.
    while let link = current, links.count < 8 {
      links.append(link)
      current = link.userInfo[NSUnderlyingErrorKey] as? NSError
    }
    return links
  }

  /// A one-line rendering of the whole chain, for `EngineError.unknown` and the log stream —
  /// never for the screen (PRD §8.6).
  ///
  /// Domain, code, and the framework's own description only. The chain's `userInfo` is
  /// deliberately not dumped: it carries `NSErrorFailingURLKey`, and a stream locator can embed a
  /// token in its query string, which TECH_SPEC §12 keeps out of log messages by construction.
  static func diagnostic(for error: NSError) -> String {
    chain(from: error)
      .map { "\($0.domain) \($0.code): \($0.localizedDescription)" }
      .joined(separator: " <- ")
  }

  private static func classify(_ error: NSError) -> EngineError? {
    switch error.domain {
    case NSURLErrorDomain: verdict(forURLErrorCode: error.code)
    case AVFoundationErrorDomain: verdict(forAVErrorCode: error.code)
    case coreMediaErrorDomain: verdict(forCoreMediaErrorCode: error.code)
    default: nil
    }
  }

  private static func verdict(forCoreMediaErrorCode raw: Int) -> EngineError? {
    switch raw {
    case Int(kCMFormatDescriptionBridgeError_InvalidParameter): .decoderFailed
    default: nil
    }
  }

  private static func verdict(forURLErrorCode raw: Int) -> EngineError? {
    switch URLError.Code(rawValue: raw) {
    case .cannotFindHost, .cannotConnectToHost, .notConnectedToInternet, .dnsLookupFailed:
      return .sourceUnreachable
    case .timedOut:
      return .timeout
    case .userAuthenticationRequired:
      return .unauthorized
    default:
      return nil
    }
  }

  /// The `AVError` groupings, taken from what each code means in `AVError.h` rather than from its
  /// name.
  ///
  /// The split that matters is `.unsupportedFormat` versus `.decoderFailed`, because both offer
  /// "Try other player" and neither should catch a code that means something else — an
  /// unclassified code falling through to `.unknown` shows the viewer an honest "wouldn't play"
  /// with no misleading button, which is a better failure than a confident wrong class.
  ///
  /// Codes that name a *container or protocol* the engine cannot demux at all are
  /// `.unsupportedFormat`; codes that name a *codec inside a container that did demux* are
  /// `.decoderFailed`. `.contentIsNotAuthorized`/`.applicationIsNotAuthorized` are the only two
  /// that mean what `.unauthorized` means — `.contentIsProtected` is deliberately absent, since
  /// it means DRM, and telling a viewer their source "refused the login" over a FairPlay stream
  /// would be a lie.
  private static func verdict(forAVErrorCode raw: Int) -> EngineError? {
    guard let code = AVError.Code(rawValue: raw) else { return nil }
    switch code {
    case .failedToParse, .fileFailedToParse, .fileFormatNotRecognized, .unsupportedOutputSettings,
      .formatUnsupported:
      return .unsupportedFormat
    case .decodeFailed, .decoderNotFound, .decoderTemporarilyUnavailable, .undecodableMediaData:
      return .decoderFailed
    case .contentIsNotAuthorized, .applicationIsNotAuthorized:
      return .unauthorized
    default:
      return nil
    }
  }

  private static let coreMediaErrorDomain = "CoreMediaErrorDomain"
}
