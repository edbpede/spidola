// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import PlayerContract
import XCTest

@testable import PlayerMPV

/// The contract's deinit backstop: dropping an engine's last reference must be equivalent to
/// `stop()`. The observable proof is the state stream finishing — it only does when teardown ran.
/// No stream is loaded, so this exercises the lifecycle without an mpv core; the loaded case
/// shares the same `stop()`, whose ordering is covered by its own documentation and the zap-path
/// acceptance run.
///
/// XCTest rather than Swift Testing to match the Apple lane: these run in the app's test bundle
/// on the simulator (`swift test` cannot run a tvOS-triple package), the same route the other
/// player suites take.
final class MPVEngineLifecycleTests: XCTestCase {
  @MainActor
  func testDroppingTheLastReferenceStopsTheEngine() async {
    let states: AsyncStream<PlaybackState>
    do {
      let engine = MPVEngine()
      states = engine.states
    }  // Last reference gone; `isolated deinit` must run `stop()`.

    // A timeout rather than a bare `for await`: on regression the stream never finishes, and the
    // failure must be a red test, not a hung lane.
    let finished = expectation(description: "states finished after the engine was dropped")
    let drain = Task { @MainActor in
      for await _ in states {}
      finished.fulfill()
    }
    await fulfillment(of: [finished], timeout: 5)
    drain.cancel()
  }
}
