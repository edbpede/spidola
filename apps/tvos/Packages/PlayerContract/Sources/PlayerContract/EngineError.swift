// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

/// The shared engine failure taxonomy (TECH_SPEC §8). Exactly the classes the PRD's error UX
/// needs — no more, so every engine maps its native failures onto a set the playback UI can
/// present without knowing which engine produced it.
///
/// The Kotlin mirror is `dev.spidola.tv.core.playercontract.EngineError`; the two must stay
/// variant-for-variant identical, since parity is the point of specifying the contract at all.
public enum EngineError: Error, Sendable, Equatable, Hashable {
  /// The stream's host could not be reached (DNS, connection refused, network down).
  case sourceUnreachable
  /// The stream's host rejected our credentials (HTTP 401/403).
  case unauthorized
  /// The container or protocol is one this engine cannot demux at all.
  case unsupportedFormat
  /// The container demuxed, but a codec inside it failed to decode.
  case decoderFailed
  /// The stream neither opened nor failed within the engine's deadline.
  case timeout
  /// Anything the engine could not classify. `detail` is diagnostic text for the log stream —
  /// never for the screen (PRD §8.6).
  case unknown(detail: String)
}

extension EngineError {
  /// Whether this failure should offer the one-button "Try other player" (TECH_SPEC §8).
  ///
  /// Only a format or decode failure means "a different engine could plausibly play this". A
  /// network or auth failure would fail identically on any engine, so offering another player
  /// there would be a lie that wastes the viewer's time.
  ///
  /// Fallback is **loud, never silent**: this only decides whether the button is offered, never
  /// whether an engine is swapped behind the viewer's back — a silent swap would make engine
  /// bugs invisible and support impossible.
  public var offersOtherPlayer: Bool {
    switch self {
    case .unsupportedFormat, .decoderFailed: true
    case .sourceUnreachable, .unauthorized, .timeout, .unknown: false
    }
  }

  /// The couch-legible failure class (PRD §6.3, §8.6 voice). No system jargon, no engine names.
  public var failureClass: String {
    switch self {
    case .sourceUnreachable: "Can't reach this channel"
    case .unauthorized: "This channel refused the login"
    case .unsupportedFormat: "This channel's format isn't supported"
    case .decoderFailed: "This channel wouldn't play"
    case .timeout: "This channel is taking too long"
    case .unknown: "This channel wouldn't play"
    }
  }

  /// A one-sentence, jargon-free explanation of what happened.
  public var message: String {
    switch self {
    case .sourceUnreachable: "The stream's server didn't answer."
    case .unauthorized: "The stream's server didn't accept this source's login."
    case .unsupportedFormat: "The other player may handle this format."
    case .decoderFailed: "The video started but couldn't be decoded."
    case .timeout: "The stream didn't start in time."
    case .unknown: "Something went wrong starting this channel."
    }
  }

  /// Diagnostic detail for the log stream only — `nil` for every classified variant, since a
  /// classified failure's diagnosis is its class.
  public var diagnosticDetail: String? {
    switch self {
    case .unknown(let detail): detail
    case .sourceUnreachable, .unauthorized, .unsupportedFormat, .decoderFailed, .timeout: nil
    }
  }
}
