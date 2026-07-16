// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import CoreKit
import Foundation
import XCTest
import core_api

@testable import FeatureSources

@MainActor
final class Phase7ModelsTests: XCTestCase {
  func testCustomEditorValidatesBeforeCrossingBoundary() async {
    let access = FakePhase7Access()
    let model = CustomChannelEditorModel(summary: nil, access: access)

    let invalidSaveSucceeded = await model.save()
    XCTAssertFalse(invalidSaveSucceeded)
    XCTAssertEqual(model.validationMessage, "Enter a channel name.")

    model.input.name = "Local camera"
    model.input.streamAddress = "https://example.invalid/live.m3u8"
    let validSaveSucceeded = await model.save()
    XCTAssertTrue(validSaveSucceeded)
    XCTAssertEqual(access.createdInputs.map(\.name), ["Local camera"])
  }

  func testGuideRefreshReportsCommittedOutcome() async {
    let access = FakePhase7Access()
    let model = GuideSettingsModel(sourceId: 7, access: access)

    await model.load()
    await model.refresh(now: Date(timeIntervalSince1970: 1_000))

    XCTAssertEqual(model.refreshStatus, .complete(inserted: 12))
  }

  func testCustomLineupMoveUsesAdjacentAnchor() async {
    let access = FakePhase7Access()
    access.ungrouped = [
      Self.channel(id: 1, name: "One"), Self.channel(id: 2, name: "Two"),
    ]
    let model = CustomChannelsModel(access: access)
    await model.load()

    await model.moveChannelDown(access.ungrouped[0])

    XCTAssertEqual(access.channelMoves, [.after(id: 1, anchor: 2)])
  }

  func testPortableSharingKeepsMergeAndReplaceExplicit() async {
    let access = FakePhase7Access()
    let model = CustomSharingModel(access: access)
    model.importContents = "{\"version\":1}"

    await model.importChannels(mode: .merge)
    model.importContents = "{\"version\":1}"
    await model.importChannels(mode: .replace)

    XCTAssertEqual(access.importModes, [.merge, .replace])
  }

  private static func channel(id: Int64, name: String) -> CustomChannelSummary {
    CustomChannelSummary(
      id: id, groupId: nil, name: name, logo: nil, hasUserAgent: false, headerCount: 0,
      position: id)
  }
}

@MainActor
private final class FakePhase7Access: EpgAccess, CustomChannelsAccess {
  enum ChannelMove: Equatable {
    case before(id: Int64, anchor: Int64)
    case after(id: Int64, anchor: Int64)
  }

  var groups: [CustomGroup] = []
  var ungrouped: [CustomChannelSummary] = []
  var grouped: [Int64: [CustomChannelSummary]] = [:]
  var createdInputs: [CustomChannelInput] = []
  var channelMoves: [ChannelMove] = []
  var importModes: [CustomImportMode] = []

  func nowNext(sourceId: Int64, channelIdentity: Int64, now: Date) async throws -> NowNext {
    NowNext(current: nil, next: nil)
  }

  func nowNextBatch(
    sourceId: Int64, channelIdentities: [Int64], now: Date
  ) async throws -> [ChannelNowNext] {
    channelIdentities.map {
      ChannelNowNext(channelIdentity: $0, programmes: NowNext(current: nil, next: nil))
    }
  }

  func epgWindow(_ query: EpgWindowQuery) async throws -> EpgPage {
    EpgPage(programmes: [], offset: query.offset)
  }

  func hasEpgFeed(sourceId: Int64) async throws -> Bool { true }
  func setXmltvFeed(sourceId: Int64, url: String) async throws {}
  func clearXmltvFeed(sourceId: Int64) async throws {}
  nonisolated func refreshEpg(sourceId: Int64, now: Date) -> AsyncStream<EpgRefreshEvent> {
    AsyncStream { continuation in
      continuation.yield(.progress(EpgRefreshProgress(stage: .downloading, programmesSeen: 12)))
      continuation.yield(
        .complete(EpgRefreshOutcome(inserted: 12, emitted: 12, skipped: 0, unmapped: 0)))
      continuation.finish()
    }
  }

  func customGroups() async throws -> [CustomGroup] { groups }
  func customChannels(groupId: Int64?) async throws -> [CustomChannelSummary] {
    guard let groupId else { return ungrouped }
    return grouped[groupId] ?? []
  }
  func createCustomChannel(_ input: CustomChannelInput) async throws -> Int64 {
    createdInputs.append(input)
    return 100
  }
  func updateCustomChannel(id: Int64, input: CustomChannelInput) async throws {}
  func deleteCustomChannel(id: Int64) async throws {}
  func moveCustomChannelBefore(id: Int64, anchorId: Int64) async throws {
    channelMoves.append(.before(id: id, anchor: anchorId))
  }
  func moveCustomChannelAfter(id: Int64, anchorId: Int64) async throws {
    channelMoves.append(.after(id: id, anchor: anchorId))
  }
  func createCustomGroup(name: String) async throws -> Int64 { 100 }
  func renameCustomGroup(id: Int64, name: String) async throws {}
  func deleteCustomGroup(id: Int64) async throws {}
  func moveCustomGroupBefore(id: Int64, anchorId: Int64) async throws {}
  func moveCustomGroupAfter(id: Int64, anchorId: Int64) async throws {}
  func exportCustomChannels() async throws -> String { "{\"version\":1}" }
  func importCustomChannels(_ contents: String, mode: CustomImportMode) async throws -> UInt64 {
    importModes.append(mode)
    return 2
  }
}
