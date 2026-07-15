// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import CoreKit
import PlayerContract
import XCTest
import core_api

@testable import FeaturePlayback

/// The playback slice's view-model logic against a fake CoreKit and the contract's `FakeEngine`
/// (TECH_SPEC §10) — no decoder, no network, no timing.
@MainActor
final class PlaybackModelTests: XCTestCase {
  // MARK: - Engine selection

  func testUsesPlatformDefaultWhenNoOverrides() async {
    let harness = Harness()
    let model = harness.model()
    await model.start()
    XCTAssertEqual(harness.built.map(\.rawValue), ["mpv"])
  }

  // MARK: - Play-time stream resolution

  func testPlaysTheResolvedLocatorRatherThanTheStoredOne() async {
    // An Xtream catalog stores a credential-free locator (TECH_SPEC §12), so the row the viewer
    // picked is not playable on its own. Without this the engine is handed an address that cannot
    // open, and the failure looks like a broken stream rather than a missing step.
    let harness = Harness()
    let model = harness.model()
    await model.start()
    XCTAssertEqual(
      harness.engines.first?.loaded.map(\.locator), ["resolved://http://host.example/10.ts"],
      "the engine must be loaded with the resolver's URL, not the stored locator")
    XCTAssertEqual(
      harness.access.resolvedCalls, ["http://host.example/10.ts"],
      "resolution must be asked for exactly once per load, not cached")
  }

  func testFallsBackToTheStoredLocatorWhenResolutionFails() async {
    // For an M3U source the two are identical, so the fallback is exact; for an Xtream source the
    // engine then fails with its own EngineError — the loud, actionable path (PRD §6.3) — rather
    // than this silently loading nothing.
    let harness = Harness()
    harness.access.resolveFailure = FakeResolveError.unavailable
    let model = harness.model()
    await model.start()
    XCTAssertEqual(
      harness.engines.first?.loaded.map(\.locator), ["http://host.example/10.ts"],
      "a failed resolution must still hand the engine the stored locator")
  }

  func testHonoursChannelOverrideOverSource() async {
    let harness = Harness()
    harness.access.channelEngines["1-10"] = "avplayer"
    harness.access.sourceEngines[1] = "mpv"
    let model = harness.model()
    await model.start()
    XCTAssertEqual(harness.built.map(\.rawValue), ["avplayer"])
  }

  func testHonoursSourceOverrideWhenNoChannelOverride() async {
    let harness = Harness()
    harness.access.sourceEngines[1] = "avplayer"
    let model = harness.model()
    await model.start()
    XCTAssertEqual(harness.built.map(\.rawValue), ["avplayer"])
  }

  /// A stale key from another platform must not make a channel unplayable.
  func testStaleOverrideFallsBackToDefault() async {
    let harness = Harness()
    harness.access.channelEngines["1-10"] = "exoplayer"
    let model = harness.model()
    await model.start()
    XCTAssertEqual(harness.built.map(\.rawValue), ["mpv"])
  }

  /// A composition bug must surface as one honest failure, not a blank screen.
  func testUnregisteredDefaultReportsEngineUnavailable() async {
    let harness = Harness(registered: [])
    let model = harness.model()
    await model.start()
    XCTAssertTrue(model.engineUnavailable)
    XCTAssertNotNil(model.state.failure)
  }

  // MARK: - Loud fallback

  func testFormatFailureOffersOtherPlayer() async {
    let harness = Harness()
    let model = harness.model()
    await model.start()
    harness.engines[0].simulate(.failed(.unsupportedFormat))
    await settle()
    XCTAssertEqual(model.fallbackOffer?.alternate, .avPlayer)
  }

  func testDecoderFailureOffersOtherPlayer() async {
    let harness = Harness()
    let model = harness.model()
    await model.start()
    harness.engines[0].simulate(.failed(.decoderFailed))
    await settle()
    XCTAssertEqual(model.fallbackOffer?.alternate, .avPlayer)
  }

  /// A network failure would fail identically on any engine — offering a swap would be a lie.
  func testNetworkFailureDoesNotOfferOtherPlayer() async {
    let harness = Harness()
    let model = harness.model()
    await model.start()
    harness.engines[0].simulate(.failed(.sourceUnreachable))
    await settle()
    XCTAssertNil(model.fallbackOffer)
    XCTAssertEqual(model.state.failure, .sourceUnreachable)
  }

  func testUnauthorizedDoesNotOfferOtherPlayer() async {
    let harness = Harness()
    let model = harness.model()
    await model.start()
    harness.engines[0].simulate(.failed(.unauthorized))
    await settle()
    XCTAssertNil(model.fallbackOffer)
  }

  /// With nothing else registered there is nothing honest to offer.
  func testNoOfferWhenOnlyOneEngineRegistered() async {
    let harness = Harness(registered: [.mpv])
    let model = harness.model()
    await model.start()
    harness.engines[0].simulate(.failed(.unsupportedFormat))
    await settle()
    XCTAssertNil(model.fallbackOffer)
  }

  // MARK: - Try other player

  func testTryOtherPlayerRebuildsOnAlternateAndRemembers() async {
    let harness = Harness()
    let model = harness.model()
    await model.start()
    harness.engines[0].simulate(.failed(.unsupportedFormat))
    await settle()
    await model.tryOtherPlayer(remember: true)
    XCTAssertEqual(harness.built.map(\.rawValue), ["mpv", "avplayer"])
    XCTAssertEqual(harness.access.channelEngines["1-10"], "avplayer")
    XCTAssertNil(model.fallbackOffer)
  }

  /// "Just this once" must not write a preference.
  func testTryOtherPlayerWithoutRememberDoesNotPersist() async {
    let harness = Harness()
    let model = harness.model()
    await model.start()
    harness.engines[0].simulate(.failed(.unsupportedFormat))
    await settle()
    await model.tryOtherPlayer(remember: false)
    XCTAssertEqual(harness.built.map(\.rawValue), ["mpv", "avplayer"])
    XCTAssertNil(harness.access.channelEngines["1-10"])
  }

  /// The previous engine must be torn down — a leaked decoder per fallback would be fatal on TV.
  func testTryOtherPlayerDisposesTheFailedEngine() async {
    let harness = Harness()
    let model = harness.model()
    await model.start()
    harness.engines[0].simulate(.failed(.unsupportedFormat))
    await settle()
    await model.tryOtherPlayer(remember: false)
    XCTAssertTrue(harness.engines[0].isStopped)
  }

  // MARK: - Zap

  func testZapNextLoadsTheFollowingChannelAndDisposesPrevious() async {
    let harness = Harness()
    let model = harness.model()
    await model.start()
    await settle()
    await model.zap(.next)
    XCTAssertEqual(model.channel.identity, 11)
    XCTAssertTrue(harness.engines[0].isStopped)
    XCTAssertEqual(harness.built.count, 2)
  }

  /// The zap path sets the channel from the window row, bypassing the route payload's mapping — so
  /// the row's kind must arrive intact. The strip's live marker keys on it, and a ring can mix
  /// kinds (favourites and search do), so inheriting the left channel's kind would lie.
  func testZapCarriesTheWindowRowsKind() async {
    let harness = Harness()
    let model = harness.model()
    await model.start()
    await settle()
    await model.zap(.next)
    XCTAssertEqual(model.channel.kind, .movie)
  }

  func testZapPreviousAtTheStartIsANoOp() async {
    let harness = Harness()
    let model = harness.model()
    await model.start()
    await settle()
    await model.zap(.previous)
    XCTAssertEqual(model.channel.identity, 10)
    XCTAssertEqual(harness.built.count, 1)
  }

  /// A refresh can move offsets under a playing channel; the ring is dropped rather than zapping
  /// somewhere the viewer did not ask for.
  func testWindowIsDroppedWhenTheRingMovedUnderTheChannel() async {
    let harness = Harness()
    harness.access.windowIdentityOverride = 999
    let model = harness.model()
    await model.start()
    await settle()
    XCTAssertNil(model.window)
    await model.zap(.next)
    XCTAssertEqual(model.channel.identity, 10)
  }

  // MARK: - Transport

  func testStopDisposesTheEngine() async {
    let harness = Harness()
    let model = harness.model()
    await model.start()
    model.stop()
    XCTAssertTrue(harness.engines[0].isStopped)
    XCTAssertNil(model.engine)
  }

  /// Back stops the model without cancelling the view's `.task` (`PlaybackView.exit`), so a load
  /// suspended in a core lookup has to notice the stop itself. Building an engine after it would
  /// leave playback running with no screen and no owner left to stop it.
  func testStopDuringLoadNeverStartsAnEngine() async {
    let harness = Harness()
    harness.access.holdsChannelEngineLookup = true
    let model = harness.model()
    let start = Task { await model.start() }
    await settle()
    model.stop()
    harness.access.releaseChannelEngineLookup()
    await start.value
    XCTAssertTrue(harness.built.isEmpty)
    XCTAssertNil(model.engine)
  }

  /// The disappear path, where `.task` cancellation can land before `onDisappear` calls `stop` and
  /// so leaves the generation untouched.
  func testCancelledLoadNeverStartsAnEngine() async {
    let harness = Harness()
    harness.access.holdsChannelEngineLookup = true
    let model = harness.model()
    let start = Task { await model.start() }
    await settle()
    start.cancel()
    harness.access.releaseChannelEngineLookup()
    await start.value
    XCTAssertTrue(harness.built.isEmpty)
    XCTAssertNil(model.engine)
  }

  func testRecordsARecentOnPlay() async {
    let harness = Harness()
    let model = harness.model()
    await model.start()
    await settle()
    XCTAssertEqual(harness.access.recorded.map(\.identity), [10])
  }

  /// Lets the model's detached window/recents tasks and the engine's state stream run.
  private func settle() async {
    for _ in 0..<8 { await Task.yield() }
  }
}

// MARK: - Harness

@MainActor
private final class Harness {
  let access = FakePlaybackAccess()
  private(set) var built: [EngineID] = []
  private(set) var engines: [FakeEngine] = []
  private let registered: Set<EngineID>

  init(registered: Set<EngineID> = [.mpv, .avPlayer]) {
    self.registered = registered
  }

  func model() -> PlaybackModel {
    PlaybackModel(
      channel: Self.channel(identity: 10, name: "BBC One"),
      context: .group(sourceId: 1, kind: .live, group: "News"),
      offset: 0,
      access: access,
      registry: registry())
  }

  private func registry() -> EngineRegistry {
    var factories: [EngineID: @MainActor () -> any PlaybackEngine] = [:]
    for id in registered {
      factories[id] = { [weak self] in
        let engine = FakeEngine(id: id)
        self?.built.append(id)
        self?.engines.append(engine)
        return engine
      }
    }
    return EngineRegistry(platformDefault: .mpv, factories: factories)
  }

  static func channel(identity: Int64, name: String, kind: MediaKind = .live) -> PlayableChannel {
    PlayableChannel(
      sourceId: 1, identity: identity, name: name, group: "News", logo: nil,
      locator: "http://host.example/\(identity).ts", kind: kind)
  }
}

/// `@MainActor` rather than `@unchecked Sendable`: the tests drive it from the main actor, so the
/// isolation is real and the compiler checks it (the rules ban asserting Sendability away).
@MainActor
private final class FakePlaybackAccess: PlaybackAccess {
  var channelEngines: [String: String] = [:]
  var sourceEngines: [Int64: String] = [:]
  var recorded: [PlayableChannel] = []
  var buffering: String?
  /// Locators the model asked to have resolved, in order.
  var resolvedCalls: [String] = []
  /// Makes resolution fail, standing in for a source deleted or a secret gone from the keychain.
  var resolveFailure: (any Error)?
  /// Forces the window's current row to a different identity, as a refresh would.
  var windowIdentityOverride: Int64?
  /// Holds `channelEngine` mid-flight. The other lookups return without ever suspending, so this is
  /// the only way a test can stand where the core does and drive an exit into a load in flight.
  var holdsChannelEngineLookup = false
  private var heldChannelEngine: CheckedContinuation<Void, Never>?

  func zapWindow(context: ZapContext, offset: UInt32) async throws -> ZapWindow? {
    let identity = windowIdentityOverride ?? Int64(10 + offset)
    return ZapWindow(
      previous: offset == 0 ? nil : Harness.channel(identity: Int64(9 + offset), name: "Prev"),
      current: Harness.channel(identity: identity, name: "Current"),
      // A kind that differs from the playing channel's, so a test can prove a zap target's kind
      // comes from the window row rather than surviving from the channel left behind.
      next: Harness.channel(identity: Int64(11 + offset), name: "Next", kind: .movie),
      offset: offset,
      total: 24)
  }

  func channelEngine(sourceId: Int64, identity: Int64) async throws -> String? {
    if holdsChannelEngineLookup {
      await withCheckedContinuation { (continuation: CheckedContinuation<Void, Never>) in
        heldChannelEngine = continuation
      }
    }
    return channelEngines["\(sourceId)-\(identity)"]
  }

  /// Clears the hold as well as resuming, so a lookup that has not arrived yet cannot strand the
  /// test at the gate.
  func releaseChannelEngineLookup() {
    holdsChannelEngineLookup = false
    heldChannelEngine?.resume()
    heldChannelEngine = nil
  }

  func setChannelEngine(sourceId: Int64, identity: Int64, engine: String?) async throws {
    channelEngines["\(sourceId)-\(identity)"] = engine
  }

  func sourceEngine(sourceId: Int64) async throws -> String? { sourceEngines[sourceId] }

  /// Stands in for the core's play-time credential resolution. Prefixed rather than echoed so a
  /// test can tell a resolved locator from a stored one, which is the only way to prove the model
  /// plays what the resolver returned instead of what the catalog holds.
  func resolveStream(sourceId: Int64, locator: String) async throws -> String {
    if let resolveFailure { throw resolveFailure }
    resolvedCalls.append(locator)
    return "resolved://\(locator)"
  }

  func bufferingProfile() async throws -> String? { buffering }

  func setBufferingProfile(_ profile: String) async throws { buffering = profile }

  func recordRecent(_ channel: PlayableChannel) async throws { recorded.append(channel) }
}

/// A resolution failure, standing in for a source deleted mid-play or a secret gone from the
/// keychain. Its identity does not matter — the model only needs to see it throw.
private enum FakeResolveError: Error {
  case unavailable
}
