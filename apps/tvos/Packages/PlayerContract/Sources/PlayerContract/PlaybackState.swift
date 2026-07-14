// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

/// The small state machine every engine emits (TECH_SPEC §8). The playback UI matches this
/// exhaustively and knows nothing about which engine produced it — that is what makes the
/// "Try other player" swap a re-render rather than a rewrite.
///
/// The Kotlin mirror is `dev.spidola.tv.core.playercontract.PlaybackState`.
public enum PlaybackState: Sendable, Equatable, Hashable {
  /// Constructed, nothing loaded.
  case idle
  /// Opening the stream: the window between `load` and the first decoded frame. The
  /// click-to-first-frame budget (PRD §9) is measured across exactly this state.
  case loading
  /// Playing, but starved — video is not advancing.
  case buffering
  /// Video is advancing.
  case playing
  /// Loaded and holding position.
  case paused
  /// The stream ended on its own (VOD run-out; a live stream reaching this means the origin
  /// closed it).
  case ended
  /// Terminal failure. The engine is spent; the shell disposes it and either offers another
  /// player or presents the error.
  case failed(EngineError)
}

extension PlaybackState {
  /// Whether video is on screen. Drives whether the shell may hide its loading treatment.
  public var isShowingVideo: Bool {
    switch self {
    case .playing, .paused, .buffering: true
    case .idle, .loading, .ended, .failed: false
    }
  }

  /// Whether the engine has reached a terminal state and should be disposed.
  public var isTerminal: Bool {
    switch self {
    case .ended, .failed: true
    case .idle, .loading, .buffering, .playing, .paused: false
    }
  }

  /// The failure this state carries, if it is a failure. Keeps the fallback decision to one line
  /// at the call site rather than a re-match.
  public var failure: EngineError? {
    switch self {
    case .failed(let error): error
    case .idle, .loading, .buffering, .playing, .paused, .ended: nil
    }
  }
}
