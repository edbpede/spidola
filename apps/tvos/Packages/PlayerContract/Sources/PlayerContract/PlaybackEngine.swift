// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import SwiftUI

/// An engine's stable identity, used as the persisted override key and the registry key.
///
/// Deliberately an opaque string rather than a contract-side enum: the core already persists
/// `preferred_engine` as an opaque key precisely so engine identities stay a shell concept
/// (TECH_SPEC §8). A closed enum here would force the contract to know its own implementors,
/// inverting the dependency — the composition root registers engines, the contract never
/// enumerates them.
public struct EngineID: RawRepresentable, Sendable, Equatable, Hashable {
  public let rawValue: String

  public init(rawValue: String) {
    self.rawValue = rawValue
  }
}

extension EngineID {
  /// The MPVKit engine — mpv-class codec breadth; the tvOS default (TECH_SPEC §8).
  /// The Android libmpv engine shares this key: same engine concept, so a per-channel override
  /// set on one platform reads correctly on the other.
  public static let mpv = EngineID(rawValue: "mpv")
  /// The AVPlayer engine — HLS-native, tvOS alternate.
  public static let avPlayer = EngineID(rawValue: "avplayer")
}

/// The playback engine contract (TECH_SPEC §8). Both platforms implement the same conceptual
/// interface so product behaviour is identical.
///
/// Engines are **disposable and cheap to re-create**, because zapping destroys and rebuilds them
/// constantly — the zap path is the performance-critical consumer and its budget is an acceptance
/// test per engine. Implementations therefore keep construction free of I/O: `load` is where work
/// starts.
///
/// `@MainActor` by design rather than by accident: every implementation wraps a UIKit- or
/// AVFoundation-backed view whose lifecycle is main-thread-only, so hoisting the isolation into
/// the contract makes the requirement compiler-checked instead of a comment. Engine internals that
/// genuinely run off-main (mpv's event loop) hop back explicitly.
@MainActor
public protocol PlaybackEngine: AnyObject {
  /// This engine's stable identity — the value persisted by an override and shown in diagnostics.
  nonisolated var id: EngineID { get }

  /// The read-only state machine. The shell's single source of playback truth; hot, and safe to
  /// consume after `load` has already advanced past a state (implementations replay the current
  /// value on subscribe, so a late consumer cannot miss a terminal `failed`).
  nonisolated var states: AsyncStream<PlaybackState> { get }

  /// The current track menu. Populated once the stream's tracks are known, so it is empty in
  /// `.loading` and meaningful from `.playing` onward.
  var tracks: TrackSelection { get }

  /// Whether this stream can seek. Live streams generally cannot; the UI hides the scrubber
  /// rather than offering a control that does nothing.
  var isSeekable: Bool { get }

  /// The video surface this engine renders into, for the playback screen to host.
  func makeSurface() -> AnyView

  /// Opens `request` and begins playback. Returns immediately; progress arrives via `states`.
  /// Calling `load` twice on one engine is not supported — dispose and rebuild instead, which is
  /// exactly what the zap path does.
  func load(_ request: StreamRequest)

  func play()
  func pause()

  /// Seeks to `seconds` from the start. A no-op when `isSeekable` is false, so the caller need
  /// not guard.
  func seek(toSeconds seconds: Double)

  func select(track: TrackID)
  /// Turns subtitles off. Distinct from `select` because "no subtitle" is not a track.
  func clearSubtitle()

  func setAspect(_ mode: AspectMode)

  /// Tears the engine down and releases its decoder. Idempotent: the shell calls it on dispose
  /// and on the terminal-state path, and neither knows whether the other ran.
  func stop()
}
