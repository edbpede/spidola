// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import SwiftUI

/// A scriptable in-memory engine for feature-code tests (TECH_SPEC §10: view models are unit
/// tested against fakes, not real decoders).
///
/// It ships in the library rather than a test target because the playback slice's tests, the
/// app's UI smoke suite, and any future engine's conformance checks all need the same fake — three
/// consumers is exactly when a shared unit is earned. It touches no media framework, so it is the
/// only engine that runs on a plain build machine.
///
/// Every state transition is driver-controlled: nothing here is timed, so a test that asserts the
/// loud-fallback path cannot flake on a decode that "usually" takes 40 ms.
@MainActor
public final class FakeEngine: PlaybackEngine {
  /// The engine identity a test resolves to. Distinct from any real engine's key, so a fake can
  /// never satisfy a policy assertion that meant a real one.
  public static let identity = EngineID(rawValue: "fake")

  public nonisolated let id: EngineID
  public nonisolated let states: AsyncStream<PlaybackState>

  public private(set) var tracks = TrackSelection()
  public private(set) var isSeekable = false

  /// What `load` was called with — the zap path's assertion surface.
  public private(set) var loaded: [StreamRequest] = []
  public private(set) var aspect: AspectMode = .fit
  public private(set) var isStopped = false
  public private(set) var playCount = 0
  public private(set) var pauseCount = 0
  public private(set) var seeks: [Double] = []

  private let continuation: AsyncStream<PlaybackState>.Continuation
  private var current: PlaybackState = .idle

  public init(id: EngineID = FakeEngine.identity) {
    self.id = id
    // Buffering unbounded: a test that drives five transitions before awaiting the stream must
    // observe all five. A dropping policy would make assertions depend on consumer timing.
    let (stream, continuation) = AsyncStream<PlaybackState>.makeStream(
      bufferingPolicy: .unbounded)
    self.states = stream
    self.continuation = continuation
    continuation.yield(.idle)
  }

  public func makeSurface() -> AnyView {
    AnyView(Color.black)
  }

  public func load(_ request: StreamRequest) {
    loaded.append(request)
    emit(.loading)
  }

  public func play() {
    playCount += 1
    emit(.playing)
  }

  public func pause() {
    pauseCount += 1
    emit(.paused)
  }

  public func seek(toSeconds seconds: Double) {
    guard isSeekable else { return }
    seeks.append(seconds)
  }

  public func select(track: TrackID) {
    guard let match = tracks.available.first(where: { $0.id == track }) else { return }
    switch match.kind {
    case .audio:
      tracks = TrackSelection(
        available: tracks.available, selectedAudio: track,
        selectedSubtitle: tracks.selectedSubtitle)
    case .subtitle:
      tracks = TrackSelection(
        available: tracks.available, selectedAudio: tracks.selectedAudio, selectedSubtitle: track)
    }
  }

  public func clearSubtitle() {
    tracks = TrackSelection(
      available: tracks.available, selectedAudio: tracks.selectedAudio, selectedSubtitle: nil)
  }

  public func setAspect(_ mode: AspectMode) {
    aspect = mode
  }

  public func stop() {
    guard !isStopped else { return }
    isStopped = true
    continuation.finish()
  }

  /// The deinit backstop the contract requires of every engine. The fake holds no decoder, but
  /// keeping the backstop means tests exercise the same lifecycle the real engines have — a test
  /// draining `states` ends when the engine is dropped, exactly as it would against a real one.
  isolated deinit {
    stop()
  }

  // MARK: - Test driving

  /// Drives the engine to `state`, as a real engine's event stream would.
  public func simulate(_ state: PlaybackState) {
    emit(state)
  }

  /// Publishes a track menu, as a real engine does once the stream's tracks are known.
  public func simulateTracks(_ selection: TrackSelection, seekable: Bool = false) {
    tracks = selection
    isSeekable = seekable
  }

  private func emit(_ state: PlaybackState) {
    guard !isStopped else { return }
    current = state
    continuation.yield(state)
    if state.isTerminal { continuation.finish() }
  }

  /// The last state emitted, for tests that assert without draining the stream.
  public var currentState: PlaybackState { current }
}
