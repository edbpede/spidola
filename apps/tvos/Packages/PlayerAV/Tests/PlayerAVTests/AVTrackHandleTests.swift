// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import AVFoundation
import PlayerContract
import XCTest

@testable import PlayerAV

/// The `TrackID` ⇄ `AVMediaSelectionOption` addressing scheme.
///
/// The round-trip is the whole contract of this type: a `TrackID` that decodes to a different
/// option than it encoded would switch the viewer to the wrong language, silently and
/// plausibly — the kind of bug that survives a demo.
final class AVTrackHandleTests: XCTestCase {

  func testEveryHandleRoundTripsThroughItsTrackID() {
    for group in AVTrackHandle.Group.allCases {
      for index in [0, 1, 7, 42, 999] {
        let handle = AVTrackHandle(group: group, optionIndex: index)
        XCTAssertEqual(
          AVTrackHandle(trackID: handle.trackID), handle,
          "\(handle.trackID.rawValue) did not round-trip")
      }
    }
  }

  func testEncodingIsTheDocumentedGroupColonIndexForm() {
    XCTAssertEqual(AVTrackHandle(group: .audible, optionIndex: 0).trackID.rawValue, "audible:0")
    XCTAssertEqual(AVTrackHandle(group: .legible, optionIndex: 2).trackID.rawValue, "legible:2")
  }

  /// Distinctness matters more than the exact spelling: two options must never share a handle.
  func testHandlesAreDistinctAcrossGroupsAtTheSameIndex() {
    XCTAssertNotEqual(
      AVTrackHandle(group: .audible, optionIndex: 1).trackID,
      AVTrackHandle(group: .legible, optionIndex: 1).trackID)
  }

  /// A `TrackID` minted by the mpv engine can reach this one through a stale menu. Decoding must
  /// miss rather than resolve to an unrelated option.
  func testForeignAndMalformedTrackIDsDecodeToNil() {
    let rejected = [
      "", "audible", "1", ":", ":1", "audible:", "audible:x", "audible:-1", "audible:1:2",
      "video:0", "AUDIBLE:0", "mpv-track-3", "legible:1.5", " audible:0",
    ]
    for raw in rejected {
      XCTAssertNil(
        AVTrackHandle(trackID: TrackID(rawValue: raw)), "\"\(raw)\" should not decode")
    }
  }

  func testGroupsMapToTheContractsTrackKinds() {
    XCTAssertEqual(AVTrackHandle.Group.audible.kind, .audio)
    XCTAssertEqual(AVTrackHandle.Group.legible.kind, .subtitle)
  }

  func testGroupsMapToAVFoundationsSelectionCharacteristics() {
    XCTAssertEqual(AVTrackHandle.Group.audible.characteristic, .audible)
    XCTAssertEqual(AVTrackHandle.Group.legible.characteristic, .legible)
  }

  /// `TrackKind` has no video case, so neither may the addressable groups — an arm the contract
  /// cannot express would be an untested arm.
  func testOnlyTheContractsTwoKindsAreAddressable() {
    XCTAssertEqual(Set(AVTrackHandle.Group.allCases.map(\.kind)), Set(TrackKind.allCases))
  }
}
