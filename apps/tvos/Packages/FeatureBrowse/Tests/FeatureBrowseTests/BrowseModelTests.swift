// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import CoreKit
import FeatureBrowse
import XCTest
import core_api

@MainActor
final class BrowseModelTests: XCTestCase {
  func testStartsLoading() {
    let model = BrowseModel(catalog: FakeCatalog(response: .empty))

    guard case .loading = model.state else {
      XCTFail("Expected loading state")
      return
    }
  }

  func testLoadsEmptyState() async {
    let model = BrowseModel(catalog: FakeCatalog(response: .empty))

    await model.load()

    guard case .empty = model.state else {
      XCTFail("Expected empty state")
      return
    }
  }

  func testLoadsReadyState() async {
    let channel = Channel(
      id: 7,
      sourceId: 1,
      identity: 42,
      name: "Fixture News",
      groupTitle: "Fixture",
      logo: nil,
      locator: "https://example.invalid/live.ts",
      kind: .live,
      categoryId: nil,
      overrides: ChannelOverrides(userAgent: nil, headers: [], preferredEngine: nil)
    )
    let model = BrowseModel(catalog: FakeCatalog(response: .ready([channel])))

    await model.load()

    guard case .ready(let items) = model.state else {
      XCTFail("Expected ready state")
      return
    }
    XCTAssertEqual(items, [ChannelItem(id: 42, name: "Fixture News", group: "Fixture")])
  }

  func testLoadsErrorState() async {
    let model = BrowseModel(catalog: FakeCatalog(response: .failure))

    await model.load()

    guard case .error(let message) = model.state else {
      XCTFail("Expected error state")
      return
    }
    XCTAssertEqual(message, "Couldn't load channels — try again.")
  }
}

private struct FakeCatalog: CatalogAccess {
  enum Response: Sendable {
    case empty
    case ready([Channel])
    case failure
  }

  let response: Response

  func sources() async throws -> [Source] {
    switch response {
    case .empty:
      []
    case .ready:
      [
        .m3uFile(
          id: 1,
          common: SourceCommon(name: "Fixture Catalog", enabled: true, autoRefreshSecs: nil)
        )
      ]
    case .failure:
      throw FakeCatalogError.failed
    }
  }

  func page(sourceId: Int64, offset: UInt32, limit: UInt32) async throws -> ChannelPage {
    switch response {
    case .ready(let channels):
      ChannelPage(channels: channels, offset: offset, total: UInt64(channels.count))
    case .empty:
      ChannelPage(channels: [], offset: offset, total: 0)
    case .failure:
      throw FakeCatalogError.failed
    }
  }
}

private enum FakeCatalogError: Error {
  case failed
}
