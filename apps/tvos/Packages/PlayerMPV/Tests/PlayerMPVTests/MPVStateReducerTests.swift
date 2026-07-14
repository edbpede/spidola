// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import PlayerContract
import XCTest

@testable import PlayerMPV

/// The facts-to-state machine (TECH_SPEC §8).
///
/// mpv's flags arrive independently and in any order, so the precedence between them is the whole
/// behaviour — and precedence is exactly what a pure reducer lets us pin down without a decoder.
final class MPVStateReducerTests: XCTestCase {

  /// A file that is open and advancing: every flag says "nothing is wrong".
  private var playing: MPVPlaybackFacts {
    MPVPlaybackFacts(
      fileLoaded: true, pausedForCache: false, paused: false, coreIdle: false, eofReached: false)
  }

  func testFreshEngineIsLoading() {
    XCTAssertEqual(MPVStateReducer.state(from: MPVPlaybackFacts()), .loading)
  }

  func testOpenAndAdvancingIsPlaying() {
    XCTAssertEqual(MPVStateReducer.state(from: playing), .playing)
  }

  /// `core-idle` is true throughout a load. If this arm did not precede it, every load would report
  /// `.buffering` before its first frame and the click-to-first-frame window (PRD §9) would be
  /// measured against the wrong state.
  func testNotYetLoadedIsLoadingEvenThoughCoreIsIdle() {
    var facts = MPVPlaybackFacts()
    facts.coreIdle = true
    XCTAssertEqual(MPVStateReducer.state(from: facts), .loading)
  }

  func testPausedForCacheIsBuffering() {
    var facts = playing
    facts.pausedForCache = true
    facts.coreIdle = true
    XCTAssertEqual(MPVStateReducer.state(from: facts), .buffering)
  }

  /// Pausing sets `core-idle` too. Someone who pressed pause should see a paused player, not a
  /// spinner.
  func testPausedBeatsCoreIdle() {
    var facts = playing
    facts.paused = true
    facts.coreIdle = true
    XCTAssertEqual(MPVStateReducer.state(from: facts), .paused)
  }

  /// Pausing during a stall sets both flags. The viewer's own action wins — a spinner over a
  /// deliberate pause reads as a bug.
  func testPausedBeatsPausedForCache() {
    var facts = playing
    facts.paused = true
    facts.pausedForCache = true
    XCTAssertEqual(MPVStateReducer.state(from: facts), .paused)
  }

  /// The classic mpv wiring bug: reporting `.playing` while the core is idle hides the spinner over
  /// a frozen frame.
  func testCoreIdleWithoutPauseIsBufferingNotPlaying() {
    var facts = playing
    facts.coreIdle = true
    XCTAssertEqual(MPVStateReducer.state(from: facts), .buffering)
  }

  func testEOFIsEnded() {
    var facts = playing
    facts.eofReached = true
    XCTAssertEqual(MPVStateReducer.state(from: facts), .ended)
  }

  /// End-of-stream is the last word. mpv leaves other flags set as it winds down, and any of them
  /// winning here would strand the shell on a state it never leaves.
  func testEOFBeatsEveryOtherFact() {
    let facts = MPVPlaybackFacts(
      fileLoaded: true, pausedForCache: true, paused: true, coreIdle: true, eofReached: true)
    XCTAssertEqual(MPVStateReducer.state(from: facts), .ended)
  }

  /// The reducer is total and pure: same facts, same state, no hidden inputs.
  func testReducerIsDeterministic() {
    let facts = playing
    XCTAssertEqual(MPVStateReducer.state(from: facts), MPVStateReducer.state(from: facts))
  }
}
