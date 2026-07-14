// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import CoreKit
import Foundation
import OSLog
import PlayerContract

/// The offer shown when the default engine hit a format/decode failure (TECH_SPEC §8).
///
/// Fallback is **loud, never silent**: this value exists so the viewer chooses, and its presence is
/// the only way an engine ever changes mid-channel. A silent swap would make engine bugs invisible.
public struct FallbackOffer: Sendable, Equatable {
  public let error: EngineError
  /// The engine "Try other player" would use.
  public let alternate: EngineID
}

/// The playback screen's state (TECH_SPEC §6: view models are `@Observable`, unit-tested against a
/// fake CoreKit and the contract's `FakeEngine`).
///
/// It owns the engine's whole life: resolve by policy → load → run → dispose. Zapping disposes and
/// rebuilds, because engines are single-use by contract and that is the path the channel-zapper
/// persona lives in (PRD §8.5) — so it is kept free of anything that could stall a rebuild.
@MainActor
@Observable
public final class PlaybackModel {
  public private(set) var state: PlaybackState = .idle
  /// The playing channel and its neighbours — the channel strip's peek and the zap ends.
  public private(set) var window: ZapWindow?
  public private(set) var channel: PlayableChannel
  public private(set) var tracks = TrackSelection()
  public private(set) var isSeekable = false
  public private(set) var aspect: AspectMode = .fit
  public private(set) var fallbackOffer: FallbackOffer?
  /// Set when the resolved engine could not be built — a composition bug, surfaced honestly
  /// rather than as a blank screen.
  public private(set) var engineUnavailable = false

  /// The live engine. The view hosts its surface; nothing else reaches through it.
  public private(set) var engine: (any PlaybackEngine)?

  private let access: any PlaybackAccess
  private let registry: EngineRegistry
  private let context: ZapContext
  private var offset: UInt32
  private var stateTask: Task<Void, Never>?
  private var loadStartedAt: ContinuousClock.Instant?
  private let clock = ContinuousClock()
  private let logger = Logger(subsystem: "dev.spidola.tv", category: "spidola::playback")

  public init(
    channel: PlayableChannel,
    context: ZapContext,
    offset: UInt32,
    access: any PlaybackAccess,
    registry: EngineRegistry
  ) {
    self.channel = channel
    self.context = context
    self.offset = offset
    self.access = access
    self.registry = registry
  }

  /// Resolves the engine by policy and starts the stream. The recents record and the zap window
  /// are deliberately *not* awaited before `load`: click-to-first-frame is budgeted at two seconds
  /// (PRD §9) and neither is needed to put video on screen.
  public func start() async {
    await play(channel, engineOverride: nil)
    async let _ = loadWindow()
    async let _ = recordRecent()
  }

  /// Zaps to an adjacent channel — the sacred path (TECH_SPEC §11). Tears the engine down and
  /// rebuilds, which is exactly what the contract's single-use engines are designed for.
  public func zap(_ direction: ZapDirection) async {
    guard let target = direction.channel(in: window) else { return }
    let nextOffset = direction.offset(from: offset)
    offset = nextOffset
    channel = target
    // The window is refreshed after the stream is loading, not before: the peek is cosmetic and
    // must never sit between a D-pad press and video.
    await play(target, engineOverride: nil)
    async let _ = loadWindow()
    async let _ = recordRecent()
  }

  /// Accepts the loud-fallback offer, optionally remembering the choice for this channel.
  public func tryOtherPlayer(remember: Bool) async {
    guard let offer = fallbackOffer else { return }
    fallbackOffer = nil
    if remember {
      await rememberEngine(offer.alternate)
    }
    await play(channel, engineOverride: offer.alternate)
  }

  public func dismissFallback() {
    fallbackOffer = nil
  }

  // MARK: - Transport

  public func togglePause() {
    guard let engine else { return }
    switch state {
    case .playing, .buffering: engine.pause()
    case .paused: engine.play()
    case .idle, .loading, .ended, .failed: break
    }
  }

  public func seek(by seconds: Double) {
    guard let engine, isSeekable else { return }
    engine.seek(toSeconds: seconds)
  }

  public func select(track: TrackID) {
    engine?.select(track: track)
    tracks = engine?.tracks ?? tracks
  }

  public func clearSubtitle() {
    engine?.clearSubtitle()
    tracks = engine?.tracks ?? tracks
  }

  public func cycleAspect() {
    aspect = aspect.next
    engine?.setAspect(aspect)
  }

  /// Disposes the engine. The view calls this on disappear; `stop` is idempotent by contract, so
  /// it is safe alongside the terminal-state path.
  public func stop() {
    stateTask?.cancel()
    stateTask = nil
    engine?.stop()
    engine = nil
  }

  // MARK: - Engine lifecycle

  private func play(_ target: PlayableChannel, engineOverride: EngineID?) async {
    stop()
    fallbackOffer = nil
    engineUnavailable = false
    state = .loading

    let resolved = await resolveEngine(target, override: engineOverride)
    guard let built = registry.make(resolved) else {
      // Only reachable when the platform default itself is not registered — a wiring bug. Report
      // it as one honest failure rather than substituting an engine the policy did not choose.
      logger.error("no engine registered for \(resolved.rawValue, privacy: .public)")
      engineUnavailable = true
      state = .failed(.unknown(detail: "engine \(resolved.rawValue) not registered"))
      return
    }

    engine = built
    built.setAspect(aspect)
    observe(built, id: resolved)
    loadStartedAt = clock.now
    logger.info(
      "load channel \(target.identity) on \(resolved.rawValue, privacy: .public)")
    built.load(await request(for: target))
    built.play()
  }

  private func resolveEngine(_ target: PlayableChannel, override: EngineID?) async -> EngineID {
    if let override { return override }
    // Overrides are opaque strings in the core (engine identity is a shell concept, TECH_SPEC §8);
    // the mapping to `EngineID` happens here, where both layers are in scope.
    let channelKey = try? await access.channelEngine(
      sourceId: target.sourceId, identity: target.identity)
    let sourceKey = try? await access.sourceEngine(sourceId: target.sourceId)
    return EngineSelection.resolve(
      channelOverride: channelKey.flatMap { $0 }.map { EngineID(rawValue: $0) },
      sourceOverride: sourceKey.flatMap { $0 }.map { EngineID(rawValue: $0) },
      platformDefault: registry.platformDefault,
      registered: registry.registered)
  }

  private func request(for target: PlayableChannel) async -> StreamRequest {
    let profile =
      (try? await access.bufferingProfile())
      .flatMap { $0 }
      .flatMap(BufferingProfile.init(rawValue:)) ?? .balanced
    return StreamRequest(locator: target.locator, buffering: profile)
  }

  /// Drains the engine's state machine onto the model. One task per engine; cancelled on dispose,
  /// so a zapped-away engine cannot write state for a channel the viewer already left.
  private func observe(_ built: any PlaybackEngine, id: EngineID) {
    let states = built.states
    stateTask = Task { [weak self] in
      for await next in states {
        guard let self else { return }
        apply(next, from: built, id: id)
      }
    }
  }

  private func apply(_ next: PlaybackState, from built: any PlaybackEngine, id: EngineID) {
    // A late event from a disposed engine (teardown races the event thread) must never move the
    // state of the channel now playing.
    guard engine === built else { return }
    state = next
    tracks = built.tracks
    isSeekable = built.isSeekable

    if next.isShowingVideo, let started = loadStartedAt {
      let elapsed = clock.now - started
      loadStartedAt = nil
      // The click-to-first-frame budget (PRD §9). Logged every time rather than sampled: the zap
      // path is profiled every release, and a budget you cannot see is a budget you do not keep.
      logger.info(
        """
        first frame on \(id.rawValue, privacy: .public) in \
        \(elapsed.milliseconds, privacy: .public) ms \
        (budget \(Self.firstFrameBudgetMilliseconds, privacy: .public) ms)
        """)
      if elapsed > Self.firstFrameBudget {
        logger.warning(
          "first frame exceeded budget on \(id.rawValue, privacy: .public)")
      }
    }

    if let error = next.failure {
      logger.error(
        """
        \(id.rawValue, privacy: .public) failed: \(error.failureClass, privacy: .public) \
        \(error.diagnosticDetail ?? "", privacy: .public)
        """)
      offerFallbackIfSensible(for: error, from: id)
    }
  }

  /// The loud-fallback rule (TECH_SPEC §8): offer another engine only when one could plausibly
  /// succeed, and only when there is another engine to offer.
  private func offerFallbackIfSensible(for error: EngineError, from id: EngineID) {
    guard error.offersOtherPlayer,
      let alternate = EngineSelection.alternate(to: id, registered: registry.registered)
    else { return }
    fallbackOffer = FallbackOffer(error: error, alternate: alternate)
  }

  private func rememberEngine(_ id: EngineID) async {
    do {
      try await access.setChannelEngine(
        sourceId: channel.sourceId, identity: channel.identity, engine: id.rawValue)
    } catch {
      logger.error("remembering engine failed: \(String(describing: error), privacy: .public)")
    }
  }

  private func loadWindow() async {
    window = try? await access.zapWindow(context: context, offset: offset)
    // A refresh can move offsets under a playing channel. Rather than zap somewhere the viewer did
    // not ask for, drop the ring and keep playing: the strip then shows no peek, which is honest.
    if let window, window.current.identity != channel.identity {
      self.window = nil
    }
  }

  private func recordRecent() async {
    try? await access.recordRecent(channel)
  }

  private static let firstFrameBudget: Duration = .milliseconds(2000)
  private static let firstFrameBudgetMilliseconds = 2000
}

/// Which way D-pad up/down zaps (PRD §8.4).
public enum ZapDirection: Sendable {
  case previous
  case next

  func channel(in window: ZapWindow?) -> PlayableChannel? {
    switch self {
    case .previous: window?.previous
    case .next: window?.next
    }
  }

  func offset(from current: UInt32) -> UInt32 {
    switch self {
    case .previous: current == 0 ? 0 : current - 1
    case .next: current + 1
    }
  }
}

extension Duration {
  fileprivate var milliseconds: Int64 {
    components.seconds * 1000 + components.attoseconds / 1_000_000_000_000_000
  }
}
