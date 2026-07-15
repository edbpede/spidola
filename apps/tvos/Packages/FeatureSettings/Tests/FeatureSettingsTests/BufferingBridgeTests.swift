// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import PlayerContract
import XCTest
import core_api

@testable import CoreKit
@testable import FeatureSettings

/// Pins the one seam between the buffering setting and the engine that honours it.
///
/// The core's `BufferingProfile` and `PlayerContract`'s carry the same cases and the same
/// spellings, but they are two types that cannot be one: CoreKit must not import PlayerContract
/// (engine tuning is the player layer's concept, TECH_SPEC §8), so `CoreKit.BufferingProfile`'s
/// `playbackKey` is where they meet, and nothing in either module can check them against each
/// other.
///
/// This suite lives in the settings slice because the settings slice is the only place that
/// depends on both, and because the property being defended is a settings one: a buffering choice
/// the viewer makes here has to arrive at the engine. If these two vocabularies drift, nothing
/// fails to compile — the setting just silently stops meaning anything, which is the exact failure
/// this catches.
final class BufferingBridgeTests: XCTestCase {
  private static let coreProfiles: [core_api.BufferingProfile] = [.low, .balanced, .generous]

  /// Every core profile survives the trip out to the raw value and back.
  func testBufferingProfileRoundTrips() {
    for profile in Self.coreProfiles {
      XCTAssertEqual(
        core_api.BufferingProfile(playbackKey: profile.playbackKey), profile,
        "\(profile) did not survive the round trip")
    }
  }

  /// Every profile the *contract* can express is reachable from the core — which is what makes
  /// `generous` a real option on the settings screen rather than a case the bridge folds away.
  func testEveryContractProfileIsReachableFromTheCore() {
    for contract in PlayerContract.BufferingProfile.allCases {
      let core = core_api.BufferingProfile(playbackKey: contract.rawValue)
      XCTAssertEqual(
        core.playbackKey, contract.rawValue,
        "\(contract) is not reachable through the core's vocabulary")
    }
  }

  /// The two vocabularies are the same size. A case on either side with no partner is drift.
  func testTheTwoVocabulariesAreTheSameSize() {
    XCTAssertEqual(Self.coreProfiles.count, PlayerContract.BufferingProfile.allCases.count)
    XCTAssertEqual(
      Set(Self.coreProfiles.map(\.playbackKey)),
      Set(PlayerContract.BufferingProfile.allCases.map(\.rawValue)))
  }

  /// A value this build does not know falls back to the shared default rather than trapping: it
  /// comes from persisted settings, so an unrecognized one means a newer app wrote it.
  func testUnknownRawValueFallsBackToTheSharedDefault() {
    let core = core_api.BufferingProfile(playbackKey: "written-by-a-newer-build")
    XCTAssertEqual(core, .balanced)
    // And `balanced` is what playback already falls back to when nothing is stored, so the
    // fallback does not change what a viewer hears.
    XCTAssertEqual(core.playbackKey, PlayerContract.BufferingProfile.balanced.rawValue)
  }

  /// The settings screen's option set is exactly the vocabulary, in the order it is offered.
  func testSettingsOffersEveryProfile() {
    XCTAssertEqual(
      SettingsField.buffering.choices.map(\.id),
      ["low", "balanced", "generous"])
  }
}
