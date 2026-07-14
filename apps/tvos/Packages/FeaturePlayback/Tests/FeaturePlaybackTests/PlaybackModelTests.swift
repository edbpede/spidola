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

  static func channel(identity: Int64, name: String) -> PlayableChannel {
    PlayableChannel(
      sourceId: 1, identity: identity, name: name, group: "News", logo: nil,
      locator: "http://host.example/\(identity).ts", kind: .live)
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
      next: Harness.channel(identity: Int64(11 + offset), name: "Next"),
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

  func bufferingProfile() async throws -> String? { buffering }

  func setBufferingProfile(_ profile: String) async throws { buffering = profile }

  func recordRecent(_ channel: PlayableChannel) async throws { recorded.append(channel) }
}
