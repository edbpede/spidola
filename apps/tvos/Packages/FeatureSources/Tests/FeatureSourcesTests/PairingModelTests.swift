// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import CoreKit
import Synchronization
import XCTest
import core_api

@testable import FeatureSources

@MainActor
final class PairingModelTests: XCTestCase {
  private static let session = PairingSession(
    url: "http://192.168.1.40:53219", port: 53219, token: "4821")

  func testStartAdvertisesTheAddressTheShellFoundAndShowsTheSession() async {
    // Ends after the script so `run()` returns; a live stream would wait for a phone forever,
    // which is right in production and a hung test here.
    let access = FakePairingAccess(session: Self.session, endsAfterScript: true)
    let model = PairingModel(access: access, resolveHost: { "192.168.1.40" })
    await model.run()

    guard case .waiting(let session) = model.state else { return XCTFail("expected waiting") }
    XCTAssertEqual(session.url, "http://192.168.1.40:53219")
    XCTAssertEqual(session.token, "4821")
    // The shell supplies the host. The core's own inference is what breaks behind a VPN, so
    // handing it `nil` is the one thing this screen must never do.
    XCTAssertEqual(access.startedHosts, ["192.168.1.40"])
  }

  /// When this TV has no dialable LAN address, the screen says so rather than asking the core to
  /// guess — the guess is exactly what fails behind a full-tunnel VPN, and it fails by advertising
  /// an address no phone can reach.
  func testNoLanAddressFailsWithoutAskingTheCoreToGuess() async {
    let access = FakePairingAccess(session: Self.session)
    let model = PairingModel(access: access, resolveHost: { nil })
    await model.run()

    guard case .failed(let error) = model.state else { return XCTFail("expected failed") }
    XCTAssertFalse(error.actions.isEmpty)
    XCTAssertTrue(access.startedHosts.isEmpty, "the server must not be started without an address")
  }

  func testSubmissionIsSurfacedForConfirmation() async {
    let submission = PairingSubmission.xtream(
      server: "http://box.example.invalid", username: "user", password: "secret")
    let access = FakePairingAccess(session: Self.session, submissions: [submission])
    let model = PairingModel(access: access, resolveHost: { "10.0.0.5" })
    await model.run()

    guard case .received(let received) = model.state else { return XCTFail("expected received") }
    XCTAssertEqual(received, submission)
  }

  /// The server's lifetime is the security model (TECH_SPEC §12): one submission is the whole job,
  /// so the stream must end there rather than leaving a listener on the LAN.
  func testTheStreamIsLeftAfterASubmission() async {
    let access = FakePairingAccess(
      session: Self.session, submissions: [.m3uUrl(url: "http://a.invalid/a.m3u")])
    let model = PairingModel(access: access, resolveHost: { "10.0.0.5" })
    await model.run()

    XCTAssertEqual(access.terminations, 1, "leaving the stream is what stops the server")
  }

  func testAFailedStartSurfacesAnActionableError() async {
    let access = FakePairingAccess(
      session: Self.session, failure: .InvalidInput(field: .address, issue: .unavailable))
    let model = PairingModel(access: access, resolveHost: { "10.0.0.5" })
    await model.run()

    guard case .failed(let error) = model.state else { return XCTFail("expected failed") }
    XCTAssertFalse(error.actions.isEmpty)
  }

  func testStopRoutesToTheCore() async {
    let access = FakePairingAccess(session: Self.session)
    let model = PairingModel(access: access, resolveHost: { "10.0.0.5" })
    await model.stop()
    XCTAssertEqual(access.stopCalls, 1)
  }

  // MARK: - LanAddress

  /// The address predicate must accept exactly what `core-pair` will take *and* a phone can dial.
  func testOnlyRfc1918AddressesAreOfferedAsTheHost() {
    for good in ["10.0.0.5", "10.255.255.254", "172.16.0.1", "172.31.255.254", "192.168.1.40"] {
      XCTAssertTrue(LanAddress.isPrivateLanAddress(good), "\(good) should be dialable")
    }
    for bad in [
      // A CGNAT tunnel address — the exact shape of the VPN case this whole path exists for.
      "100.80.166.175",
      // Just outside the 172.16/12 block on each side.
      "172.15.0.1", "172.32.0.1",
      // Public, loopback, link-local: the core would take the last two, but a phone cannot reach
      // either, so they are not addresses worth putting on screen.
      "8.8.8.8", "127.0.0.1", "169.254.10.1",
      // Not addresses at all.
      "192.168.1", "192.168.1.256", "", "hello",
    ] {
      XCTAssertFalse(LanAddress.isPrivateLanAddress(bad), "\(bad) should not be offered")
    }
  }
}

/// A fake `PairingAccess`: replays a scripted session and submissions, and records what the model
/// did with the stream.
///
/// State lives behind a `Mutex` rather than an isolation domain because a stream's `onTermination`
/// is `@Sendable` and runs wherever the stream happened to end — assuming an actor there would
/// trap, and assuming it *correctly* would only be true by luck. A lock the fake owns is the one
/// case `@unchecked Sendable` would otherwise be reached for, and `Mutex` makes it unnecessary:
/// every stored property is an immutable `Sendable` `let`, so the compiler still proves this safe.
private final class FakePairingAccess: PairingAccess {
  /// Everything the fake observed, guarded as one value so no two fields can disagree.
  struct Recorded: Sendable {
    var startedHosts: [String] = []
    var stopCalls = 0
    var terminations = 0
  }

  private let session: PairingSession
  private let submissions: [PairingSubmission]
  private let failure: ApiError?
  private let endsAfterScript: Bool
  private let recorded = Mutex(Recorded())

  var startedHosts: [String] { recorded.withLock(\.startedHosts) }
  var stopCalls: Int { recorded.withLock(\.stopCalls) }
  /// How many times a pairing stream was terminated — which is what stops the server.
  var terminations: Int { recorded.withLock(\.terminations) }

  /// - Parameter endsAfterScript: whether the stream finishes once its events are replayed.
  ///   A real pairing stream does **not** — it stays open until the screen goes away, which is the
  ///   whole lifetime model — so the default is `false` and a test that wants `run()` to return
  ///   has to say why. Leaving this `true` everywhere would hide the very property the submission
  ///   tests check: that the *model* leaves the stream, rather than the stream ending under it.
  init(
    session: PairingSession,
    submissions: [PairingSubmission] = [],
    failure: ApiError? = nil,
    endsAfterScript: Bool = false
  ) {
    self.session = session
    self.submissions = submissions
    self.failure = failure
    self.endsAfterScript = endsAfterScript
  }

  func pairing(host: String?) -> AsyncStream<PairingEvent> {
    if let host { recorded.withLock { $0.startedHosts.append(host) } }
    let session = session
    let submissions = submissions
    let failure = failure
    // `self`, not the `Mutex`: a `Mutex` is noncopyable and cannot be captured, so the reference
    // that owns it is what the closure holds.
    return AsyncStream { [self] continuation in
      if let failure {
        continuation.yield(.failed(failure))
        continuation.finish()
        return
      }
      continuation.onTermination = { _ in self.recordTermination() }
      continuation.yield(.started(session))
      for submission in submissions { continuation.yield(.submission(submission)) }
      if endsAfterScript { continuation.finish() }
    }
  }

  private func recordTermination() { recorded.withLock { $0.terminations += 1 } }

  func stopPairing() async { recorded.withLock { $0.stopCalls += 1 } }
}
