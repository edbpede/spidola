// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import CoreKit
import XCTest
import core_api

@testable import FeatureSources

@MainActor
final class SourcesModelTests: XCTestCase {
  // MARK: - AddSourceModel

  func testAddRequiresName() {
    let model = AddSourceModel(access: FakeSourcesAccess())
    model.mode = .url
    model.url = "https://example.invalid/list.m3u"
    model.submit()
    XCTAssertNotNil(model.validationMessage)
    guard case .editing = model.state else { return XCTFail("expected editing") }
  }

  func testAddUrlImportsAndSummarizes() async {
    let access = FakeSourcesAccess(
      importResult: .complete(
        ImportOutcome(inserted: 1240, duplicatesDropped: 0, emitted: 1240, skipped: 3, invalid: 0)))
    let model = AddSourceModel(access: access)
    model.name = "Home"
    model.url = "https://example.invalid/list.m3u"
    model.submit()
    await model.waitForImport()
    guard case .done(let outcome) = model.state else { return XCTFail("expected done") }
    XCTAssertEqual(outcome.inserted, 1240)
  }

  func testAddUrlFailurePresentsActionableError() async {
    let access = FakeSourcesAccess(importResult: .failed(.NetworkUnreachable))
    let model = AddSourceModel(access: access)
    model.name = "Home"
    model.url = "https://example.invalid/list.m3u"
    model.submit()
    await model.waitForImport()
    guard case .failed(let error) = model.state else { return XCTFail("expected failed") }
    XCTAssertFalse(error.actions.isEmpty)
  }

  // MARK: - SourcesModel

  func testListLoadsAndEnableDisableReloads() async {
    let access = FakeSourcesAccess(sources: [Self.source(id: 1, name: "Home", enabled: true)])
    let model = SourcesModel(access: access)
    await model.load()
    guard case .ready(let sources) = model.state else { return XCTFail("expected ready") }
    XCTAssertEqual(sources.count, 1)

    await model.setEnabled(id: 1, enabled: false)
    XCTAssertEqual(access.lastEnabled?.enabled, false)
  }

  func testDeleteReloads() async {
    let access = FakeSourcesAccess(sources: [Self.source(id: 1, name: "Home", enabled: true)])
    let model = SourcesModel(access: access)
    await model.load()
    await model.delete(id: 1)
    XCTAssertEqual(access.deletedIds, [1])
  }

  private static func source(id: Int64, name: String, enabled: Bool) -> Source {
    .m3uUrl(
      id: id,
      common: SourceCommon(name: name, enabled: enabled, autoRefreshSecs: nil),
      url: "https://example.invalid/list.m3u",
      userAgent: nil,
      acceptInvalidTls: false)
  }
}

/// A fake `SourcesAccess`: records mutations and replays a scripted import result.
private final class FakeSourcesAccess: SourcesAccess, @unchecked Sendable {
  enum ImportResult: Sendable {
    case complete(ImportOutcome)
    case failed(ApiError)
  }

  private(set) var sourcesValue: [Source]
  private let importResult: ImportResult
  private(set) var lastEnabled: (id: Int64, enabled: Bool)?
  private(set) var deletedIds: [Int64] = []
  private var nextId: Int64 = 100

  init(
    sources: [Source] = [],
    importResult: ImportResult = .complete(
      ImportOutcome(inserted: 0, duplicatesDropped: 0, emitted: 0, skipped: 0, invalid: 0))
  ) {
    self.sourcesValue = sources
    self.importResult = importResult
  }

  func sources() async throws -> [Source] { sourcesValue }

  func addM3uUrl(name: String, url: String, userAgent: String?, acceptInvalidTls: Bool)
    async throws -> Source
  {
    let id = nextId
    nextId += 1
    return .m3uUrl(
      id: id, common: SourceCommon(name: name, enabled: true, autoRefreshSecs: nil),
      url: url, userAgent: userAgent, acceptInvalidTls: acceptInvalidTls)
  }

  func addM3uFile(name: String) async throws -> Source {
    let id = nextId
    nextId += 1
    return .m3uFile(id: id, common: SourceCommon(name: name, enabled: true, autoRefreshSecs: nil))
  }

  func rename(id: Int64, name: String) async throws {}
  func setEnabled(id: Int64, enabled: Bool) async throws { lastEnabled = (id, enabled) }
  func setAutoRefresh(id: Int64, secs: UInt32?) async throws {}
  func deleteSource(id: Int64) async throws { deletedIds.append(id) }

  func importURL(id: Int64) -> AsyncStream<ImportEvent> { scriptedStream() }
  func importContent(id: Int64, content: String) -> AsyncStream<ImportEvent> { scriptedStream() }

  private func scriptedStream() -> AsyncStream<ImportEvent> {
    let result = importResult
    return AsyncStream { continuation in
      continuation.yield(.progress(ImportProgress(stage: .downloading, channelsSeen: 1)))
      switch result {
      case .complete(let outcome): continuation.yield(.complete(outcome))
      case .failed(let error): continuation.yield(.failed(error))
      }
      continuation.finish()
    }
  }
}
