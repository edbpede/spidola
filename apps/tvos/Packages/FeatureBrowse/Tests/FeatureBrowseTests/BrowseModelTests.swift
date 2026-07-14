// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import CoreKit
import FeatureBrowse
import XCTest
import core_api

@MainActor
final class BrowseModelTests: XCTestCase {
  // MARK: - HomeModel

  func testHomeEmptyWhenNoEnabledSources() async {
    let model = HomeModel(access: FakeAccess(sources: []))
    await model.load()
    guard case .empty = model.state else { return XCTFail("expected empty") }
  }

  func testHomeReadyWithFavoritesAndRecents() async {
    let access = FakeAccess(
      sources: [Self.source(id: 1, name: "Home")],
      favorites: [Self.channel(identity: 10, name: "BBC")],
      recents: [Self.recent(identity: 11, name: "CNN")])
    let model = HomeModel(access: access)
    await model.load()
    guard case .ready(let home) = model.state else { return XCTFail("expected ready") }
    XCTAssertEqual(home.favorites.map(\.name), ["BBC"])
    XCTAssertEqual(home.recents.map(\.name), ["CNN"])
  }

  func testHomeHidesRecentsWhenOffSwitchSet() async {
    let access = FakeAccess(
      sources: [Self.source(id: 1, name: "Home")],
      recents: [Self.recent(identity: 11, name: "CNN")],
      recentsEnabled: false)
    let model = HomeModel(access: access)
    await model.load()
    guard case .ready(let home) = model.state else { return XCTFail("expected ready") }
    XCTAssertTrue(home.recents.isEmpty)
  }

  func testHomeFailedSurfacesActionableError() async {
    let model = HomeModel(access: FakeAccess(sources: [], failure: .StorageCorrupt))
    await model.load()
    guard case .failed(let error) = model.state else { return XCTFail("expected failed") }
    XCTAssertFalse(error.actions.isEmpty)
  }

  // MARK: - SourceBrowseModel

  func testSourceBrowseListsGroups() async {
    let access = FakeAccess(
      sources: [Self.source(id: 1, name: "Home")],
      kinds: [.live],
      groups: [
        BrowseGroup(title: "News", channelCount: 2), BrowseGroup(title: nil, channelCount: 1),
      ])
    let model = SourceBrowseModel(sourceId: 1, access: access)
    await model.load()
    guard case .ready(let content) = model.state else { return XCTFail("expected ready") }
    XCTAssertEqual(content.groups.map(\.channelCount), [2, 1])
  }

  // MARK: - ChannelsModel

  func testChannelsToggleFavoriteAndHide() async {
    let channel = Self.channel(identity: 42, name: "Fixture")
    let access = FakeAccess(
      sources: [Self.source(id: 1, name: "Home")],
      groupChannels: [channel])
    let model = ChannelsModel(sourceId: 1, kind: .live, group: "Fixture", access: access)
    await model.load()
    guard case .ready(let rows) = model.state, let row = rows.first else {
      return XCTFail("expected ready")
    }
    XCTAssertFalse(row.isFavorite)

    await model.toggleFavorite(row)
    guard case .ready(let afterFav) = model.state else { return XCTFail("expected ready") }
    XCTAssertTrue(afterFav[0].isFavorite)

    await model.hide(afterFav[0])
    guard case .empty = model.state else {
      return XCTFail("expected empty after hiding the only row")
    }
  }

  // MARK: - Fixtures

  private static func source(id: Int64, name: String) -> Source {
    .m3uFile(id: id, common: SourceCommon(name: name, enabled: true, autoRefreshSecs: nil))
  }

  private static func channel(identity: Int64, name: String) -> Channel {
    Channel(
      id: identity, sourceId: 1, identity: identity, name: name, groupTitle: "Fixture",
      logo: nil, locator: "https://example.invalid/live.ts", kind: .live, categoryId: nil,
      overrides: ChannelOverrides(userAgent: nil, headers: [], preferredEngine: nil))
  }

  private static func recent(identity: Int64, name: String) -> Recent {
    Recent(
      sourceId: 1, identity: identity, name: name, locator: "https://example.invalid/live.ts",
      playedAtUnix: 1000, positionSecs: nil)
  }
}

/// A fake conforming to both `HomeAccess` and `BrowseAccess`, so the browse view models are tested
/// without the real core (TECH_SPEC §10).
private final class FakeAccess: HomeAccess, BrowseAccess, @unchecked Sendable {
  private let sourcesValue: [Source]
  private let favoritesValue: [Channel]
  private let recentsValue: [Recent]
  private let recentsOn: Bool
  private let kindsValue: [MediaKind]
  private let groupsValue: [BrowseGroup]
  private let groupChannelsValue: [Channel]
  private let failure: ApiError?
  private var favoriteIds: Set<Int64> = []
  private var hiddenIds: Set<Int64> = []

  init(
    sources: [Source],
    favorites: [Channel] = [],
    recents: [Recent] = [],
    recentsEnabled: Bool = true,
    kinds: [MediaKind] = [.live],
    groups: [BrowseGroup] = [],
    groupChannels: [Channel] = [],
    failure: ApiError? = nil
  ) {
    self.sourcesValue = sources
    self.favoritesValue = favorites
    self.recentsValue = recents
    self.recentsOn = recentsEnabled
    self.kindsValue = kinds
    self.groupsValue = groups
    self.groupChannelsValue = groupChannels
    self.failure = failure
  }

  private func check() throws {
    if let failure { throw failure }
  }

  func sources() async throws -> [Source] {
    try check()
    return sourcesValue
  }

  func favoriteChannels(offset: UInt32, limit: UInt32) async throws -> ChannelPage {
    try check()
    return ChannelPage(
      channels: favoritesValue, offset: offset, total: UInt64(favoritesValue.count))
  }

  func recents(limit: UInt32) async throws -> [Recent] {
    try check()
    return recentsValue
  }
  func recentsEnabled() async throws -> Bool {
    try check()
    return recentsOn
  }
  func setRecentsEnabled(_ enabled: Bool) async throws { try check() }
  func clearRecents() async throws { try check() }
  func recordRecent(_ channel: PlayableChannel) async throws { try check() }

  func kinds(sourceId: Int64) async throws -> [MediaKind] {
    try check()
    return kindsValue
  }

  func groups(sourceId: Int64, kind: MediaKind, offset: UInt32, limit: UInt32) async throws
    -> BrowseGroupPage
  {
    try check()
    return BrowseGroupPage(groups: groupsValue, offset: offset, total: UInt64(groupsValue.count))
  }

  func channelsInGroup(
    sourceId: Int64, kind: MediaKind, group: String?, offset: UInt32, limit: UInt32
  ) async throws -> ChannelPage {
    try check()
    let visible = groupChannelsValue.filter { !hiddenIds.contains($0.identity) }
    return ChannelPage(channels: visible, offset: offset, total: UInt64(visible.count))
  }

  func isFavorite(sourceId: Int64, identity: Int64) async throws -> Bool {
    try check()
    return favoriteIds.contains(identity)
  }

  func setFavorite(sourceId: Int64, identity: Int64, favorite: Bool) async throws {
    try check()
    if favorite { favoriteIds.insert(identity) } else { favoriteIds.remove(identity) }
  }

  func favoriteIdentities(sourceId: Int64) async throws -> [Int64] {
    try check()
    return Array(favoriteIds)
  }

  func isHidden(sourceId: Int64, identity: Int64) async throws -> Bool {
    try check()
    return hiddenIds.contains(identity)
  }

  func setHidden(sourceId: Int64, identity: Int64, hidden: Bool) async throws {
    try check()
    if hidden { hiddenIds.insert(identity) } else { hiddenIds.remove(identity) }
  }
}
