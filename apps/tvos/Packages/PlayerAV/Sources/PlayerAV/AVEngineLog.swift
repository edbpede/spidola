// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import OSLog
import PlayerContract

/// This engine's log surface (TECH_SPEC §4.8).
enum AVEngineLog {
  /// Subsystem is the app bundle id and the category names the engine, so one Console filter on
  /// `spidola::player::` shows both engines' stories interleaved with the core's spans.
  static let logger = Logger(subsystem: "dev.spidola.tv", category: "spidola::player::av")
}

extension StreamRequest {
  /// The header *names* this request carries, for the log stream.
  ///
  /// Names only — and note there is deliberately no sibling that renders values. Header values
  /// carry session tokens and Xtream credentials, and TECH_SPEC §12 keeps them out of log
  /// messages by construction rather than by review: making the safe rendering the *only*
  /// rendering means an engine that wanted to log values would have to reach past this and write
  /// the interpolation itself, which is a thing a reviewer can see.
  var loggableHeaderNames: String {
    headers.isEmpty ? "none" : headers.map(\.name).joined(separator: ",")
  }

  /// Whether a user-agent override applies — not which one. A user-agent is a per-source
  /// fingerprint that portals key on, so §12 keeps its value out of the log alongside the header
  /// values; whether one was set is enough to diagnose "the override didn't apply".
  var hasUserAgentOverride: Bool { userAgent != nil }
}

extension EngineError {
  /// A stable, non-localized token for the log stream.
  ///
  /// Deliberately not `failureClass`, which is couch copy: it will be reworded and translated,
  /// and a support thread reading a year-old log needs a token that survived both.
  var logLabel: String {
    switch self {
    case .sourceUnreachable: "sourceUnreachable"
    case .unauthorized: "unauthorized"
    case .unsupportedFormat: "unsupportedFormat"
    case .decoderFailed: "decoderFailed"
    case .timeout: "timeout"
    case .unknown: "unknown"
    }
  }
}

extension PlaybackState {
  /// A stable, non-localized token for the log stream.
  var logLabel: String {
    switch self {
    case .idle: "idle"
    case .loading: "loading"
    case .buffering: "buffering"
    case .playing: "playing"
    case .paused: "paused"
    case .ended: "ended"
    case .failed: "failed"
    }
  }
}
