// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import PlayerContract
import XCTest

@testable import PlayerAV

/// The `BufferingProfile` → AVPlayer knob mapping.
///
/// These assert the *trade* each profile promises the settings screen (TECH_SPEC §8), not the
/// exact numbers for their own sake: the numbers are tunable, but a profile that stopped ordering
/// correctly against its neighbours would quietly sell the viewer the opposite of what its label
/// says.
final class AVBufferingTuningTests: XCTestCase {

  func testLowStartsImmediatelyRatherThanWaitingOutAPredictedStall() {
    let tuning = AVBufferingTuning(.low)
    XCTAssertFalse(
      tuning.waitsToMinimizeStalling,
      "the fastest-start profile cannot let AVPlayer hold the first frame back")
    XCTAssertEqual(tuning.forwardBufferDuration, 1)
  }

  func testBalancedDefersTheBufferSizeToAVFoundation() {
    let tuning = AVBufferingTuning(.balanced)
    XCTAssertEqual(
      tuning.forwardBufferDuration, 0,
      "0 is preferredForwardBufferDuration's documented automatic value")
    XCTAssertTrue(tuning.waitsToMinimizeStalling)
  }

  func testGenerousBuysRunwayAndPaysForItAtStartUp() {
    let tuning = AVBufferingTuning(.generous)
    XCTAssertEqual(tuning.forwardBufferDuration, 10)
    XCTAssertTrue(tuning.waitsToMinimizeStalling)
  }

  /// The ordering the labels sell: "Fastest start" must never buffer more than "Smoothest
  /// playback".
  func testProfilesAreOrderedFastestToSmoothest() {
    let low = AVBufferingTuning(.low)
    let generous = AVBufferingTuning(.generous)
    XCTAssertLessThan(low.forwardBufferDuration, generous.forwardBufferDuration)
    XCTAssertFalse(low.waitsToMinimizeStalling)
    XCTAssertTrue(generous.waitsToMinimizeStalling)
  }

  /// Adding a profile to the contract must force a decision here rather than silently inheriting
  /// a neighbour's numbers.
  func testEveryProfileMapsToADistinctTuning() {
    let tunings = BufferingProfile.allCases.map(AVBufferingTuning.init)
    XCTAssertEqual(Set(tunings.map(\.forwardBufferDuration)).count, BufferingProfile.allCases.count)
  }

  func testNoProfileAsksForANegativeBuffer() {
    for profile in BufferingProfile.allCases {
      XCTAssertGreaterThanOrEqual(AVBufferingTuning(profile).forwardBufferDuration, 0, "\(profile)")
    }
  }
}
