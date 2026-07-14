// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import CoreKit
import XCTest
import core_api

@testable import FeatureSearch

@MainActor
final class SearchModelTests: XCTestCase {
  func testEmptyQueryIsIdle() {
    let model = SearchModel(access: FakeSearchAccess())
    model.query = "   "
    model.scheduleSearch()
    guard case .idle = model.state else { return XCTFail("expected idle") }
  }

  func testQueryProducesResults() async {
    let access = FakeSearchAccess(results: [Self.channel(identity: 1, name: "BBC News")])
    let model = SearchModel(access: access)
    model.query = "bbc"
    model.scheduleSearch()
    await model.waitForSearch()
    guard case .results(let results) = model.state else { return XCTFail("expected results") }
    XCTAssertEqual(results.channels.map(\.name), ["BBC News"])
  }

  /// The ring must name the query the core actually ran, not whatever is in the field by the time
  /// the viewer picks a row — otherwise a result opened after one more keystroke zaps a ring the
  /// viewer never saw.
  func testResultsCarryTheQueryThatProducedThem() async {
    let access = FakeSearchAccess(results: [Self.channel(identity: 1, name: "BBC News")])
    let model = SearchModel(access: access)
    model.query = "bbc "
    model.sourceFilter = 3
    model.scheduleSearch()
    await model.waitForSearch()
    guard case .results(let results) = model.state else { return XCTFail("expected results") }
    model.query = "bbc one"
    XCTAssertEqual(results.context, .search(query: "bbc", sourceId: 3, kind: nil))
  }

  func testNoMatchesIsEmpty() async {
    let model = SearchModel(access: FakeSearchAccess(results: []))
    model.query = "zzz"
    model.scheduleSearch()
    await model.waitForSearch()
    guard case .empty = model.state else { return XCTFail("expected empty") }
  }

  func testFailureSurfacesActionableError() async {
    let model = SearchModel(access: FakeSearchAccess(failure: .StorageCorrupt))
    model.query = "x"
    model.scheduleSearch()
    await model.waitForSearch()
    guard case .failed(let error) = model.state else { return XCTFail("expected failed") }
    XCTAssertFalse(error.actions.isEmpty)
  }

  private static func channel(identity: Int64, name: String) -> Channel {
    Channel(
      id: identity, sourceId: 1, identity: identity, name: name, groupTitle: nil, logo: nil,
      locator: "https://example.invalid/live.ts", kind: .live, categoryId: nil,
      overrides: ChannelOverrides(userAgent: nil, headers: [], preferredEngine: nil))
  }
}

private final class FakeSearchAccess: SearchAccess, @unchecked Sendable {
  private let results: [Channel]
  private let fuzzy: Bool
  private let failure: ApiError?

  init(results: [Channel] = [], fuzzy: Bool = false, failure: ApiError? = nil) {
    self.results = results
    self.fuzzy = fuzzy
    self.failure = failure
  }

  func sources() async throws -> [Source] { [] }

  func search(query: String, sourceId: Int64?, kind: MediaKind?, offset: UInt32, limit: UInt32)
    async throws -> SearchPage
  {
    if let failure { throw failure }
    return SearchPage(channels: results, offset: offset, fuzzy: fuzzy)
  }
}
