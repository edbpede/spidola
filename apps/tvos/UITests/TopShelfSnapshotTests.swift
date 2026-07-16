// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import Foundation
import XCTest

final class TopShelfSnapshotTests: XCTestCase {
  func testSnapshotBoundsFavoritesAndRoundTripsDisplayMetadata() throws {
    let favorites = (0..<20).map { index in
      TopShelfSnapshot.Item(
        sourceId: 7,
        identity: Int64(index),
        title: "Channel \(index)",
        group: "News",
        imageURL: URL(string: "https://images.example/\(index).png")
      )
    }
    let snapshot = TopShelfSnapshot(
      generatedAt: Date(timeIntervalSince1970: 1_234),
      favorites: favorites
    )

    XCTAssertEqual(snapshot.favorites.count, TopShelfSnapshot.maximumItemCount)
    let data = try JSONEncoder().encode(snapshot)
    XCTAssertEqual(try JSONDecoder().decode(TopShelfSnapshot.self, from: data), snapshot)
  }

  func testItemBuildsStableChannelDeepLinkWithoutPlaybackLocator() {
    let item = TopShelfSnapshot.Item(
      sourceId: 41,
      identity: 99,
      title: "Public TV",
      group: nil,
      imageURL: nil
    )

    XCTAssertEqual(item.identifier, "41-99")
    XCTAssertEqual(item.deepLink?.absoluteString, "spidola://channel/41/99")
    XCTAssertNil(item.imageURL, "an artwork-free favorite remains eligible for Top Shelf")
  }

  func testUnsupportedSnapshotVersionIsRejected() throws {
    let snapshot = TopShelfSnapshot(favorites: [])
    let encoded = try JSONEncoder().encode(snapshot)
    var document = try XCTUnwrap(JSONSerialization.jsonObject(with: encoded) as? [String: Any])
    document["version"] = Int(TopShelfSnapshot.currentVersion) + 1
    let unsupported = try JSONDecoder().decode(
      TopShelfSnapshot.self,
      from: JSONSerialization.data(withJSONObject: document)
    )

    XCTAssertFalse(unsupported.isSupported)
  }
}
