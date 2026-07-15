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
    // The source was created before the import ran, so a failed import must drop it again —
    // otherwise an empty source litters the list and a retry adds a duplicate.
    XCTAssertEqual(access.deletedIds, [100])
  }

  // MARK: - Xtream

  func testXtreamAddPassesTheAccountThroughAndImports() async {
    let access = FakeSourcesAccess(
      importResult: .complete(
        ImportOutcome(inserted: 900, duplicatesDropped: 0, emitted: 900, skipped: 0, invalid: 0)))
    let model = AddSourceModel(access: access)
    model.mode = .xtream
    model.name = "  Provider  "
    model.server = "  http://example.invalid:8080  "
    model.username = "  user  "
    model.password = " pa ss "
    model.submit()
    await model.waitForImport()

    guard case .done(let outcome) = model.state else { return XCTFail("expected done") }
    XCTAssertEqual(outcome.inserted, 900)
    let add = try? XCTUnwrap(access.xtreamAdds.first)
    XCTAssertEqual(add?.name, "Provider")
    XCTAssertEqual(add?.server, "http://example.invalid:8080")
    XCTAssertEqual(add?.username, "user")
    // The password goes through untouched: spaces are legal in one, and trimming would reject a
    // correct password with a message about the account being wrong.
    XCTAssertEqual(add?.password, " pa ss ")
  }

  func testXtreamRequiresServerUsernameAndPassword() {
    let model = AddSourceModel(access: FakeSourcesAccess())
    model.mode = .xtream
    model.name = "Provider"
    model.submit()
    XCTAssertNotNil(model.validationMessage)
    XCTAssertEqual(model.validationMessage, "Enter the server address.")

    model.server = "http://example.invalid"
    model.submit()
    XCTAssertEqual(model.validationMessage, "Enter the username.")

    model.username = "user"
    model.submit()
    XCTAssertEqual(model.validationMessage, "Enter the password.")
    guard case .editing = model.state else { return XCTFail("expected editing") }
  }

  /// The account is verified before it is stored, so a wrong password is a sentence on this screen
  /// rather than a mystery on the next refresh (PRD §6.3).
  func testXtreamRejectionSurfacesAnActionableErrorAndAddsNothing() async {
    let access = FakeSourcesAccess()
    access.addXtreamFailure = .Unauthorized
    let model = AddSourceModel(access: access)
    model.mode = .xtream
    model.name = "Provider"
    model.server = "http://example.invalid"
    model.username = "user"
    model.password = "wrong"
    model.submit()
    await model.waitForImport()

    guard case .failed(let error) = model.state else { return XCTFail("expected failed") }
    XCTAssertFalse(error.actions.isEmpty)
    // A rejected login prescribes "Edit" — the whole point being that the details can be fixed.
    XCTAssertEqual(error.primaryAction, .fixInput)
    // Nothing was created, so nothing needs cleaning up: the verification happens before the row.
    XCTAssertTrue(access.deletedIds.isEmpty)
  }

  /// `Unauthorized` prescribes `fixInput`, so "Edit" has to put the form back. Before this existed
  /// the button re-rendered the same error and the likeliest failure on the screen had no way out
  /// but Back.
  func testReturnToFormRestoresTheFieldsAfterAFailure() async {
    let access = FakeSourcesAccess()
    access.addXtreamFailure = .Unauthorized
    let model = AddSourceModel(access: access)
    model.mode = .xtream
    model.name = "Provider"
    model.server = "http://example.invalid"
    model.username = "user"
    model.password = "wrong"
    model.submit()
    await model.waitForImport()
    guard case .failed = model.state else { return XCTFail("expected failed") }

    model.returnToForm()

    guard case .editing = model.state else { return XCTFail("expected editing") }
    XCTAssertNil(model.validationMessage)
    // The details survive, so only the wrong one has to be retyped — on a remote.
    XCTAssertEqual(model.server, "http://example.invalid")
    XCTAssertEqual(model.username, "user")
  }

  // MARK: - Pairing pre-fill

  func testXtreamSubmissionPrefillsTheFormWithoutSubmitting() {
    let access = FakeSourcesAccess()
    let model = AddSourceModel(access: access)
    model.prefill(
      from: .xtream(
        server: "http://box.example.invalid:8080", username: "user", password: "secret"))

    XCTAssertEqual(model.mode, .xtream)
    XCTAssertEqual(model.server, "http://box.example.invalid:8080")
    XCTAssertEqual(model.username, "user")
    XCTAssertEqual(model.password, "secret")
    // The host stands in for a name so nobody has to type one on a remote — which is the misery
    // pairing exists to avoid.
    XCTAssertEqual(model.name, "box.example.invalid")
    // Pre-filled, never submitted: the person at the TV confirms, because anything on the LAN
    // could have posted this (PRD §6.1).
    guard case .editing = model.state else { return XCTFail("expected editing") }
    XCTAssertTrue(access.xtreamAdds.isEmpty)
  }

  func testM3uSubmissionPrefillsTheUrlForm() {
    let model = AddSourceModel(access: FakeSourcesAccess())
    model.prefill(from: .m3uUrl(url: "http://lists.example.invalid/a.m3u"))

    XCTAssertEqual(model.mode, .url)
    XCTAssertEqual(model.url, "http://lists.example.invalid/a.m3u")
    XCTAssertEqual(model.name, "lists.example.invalid")
    guard case .editing = model.state else { return XCTFail("expected editing") }
  }

  /// A submission whose URL has no host still has to leave the form usable rather than named "".
  func testSubmissionWithoutAHostStillNamesTheSource() {
    let model = AddSourceModel(access: FakeSourcesAccess())
    model.prefill(from: .m3uUrl(url: "not a url"))
    XCTAssertEqual(model.name, "Playlist")
    XCTAssertEqual(model.url, "not a url")
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

  /// One account as `addXtream` received it — a named type rather than a tuple so each field is
  /// asserted by name, which is what makes "the password went through untrimmed" legible.
  struct XtreamAdd: Equatable {
    let name: String
    let server: String
    let username: String
    let password: String
  }

  /// Every Xtream account handed to `addXtream`, exactly as the model passed it.
  private(set) var xtreamAdds: [XtreamAdd] = []
  /// What `addXtream` should throw instead of succeeding — the headend rejecting an account.
  var addXtreamFailure: ApiError?

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

  func addXtream(name: String, server: String, username: String, password: String)
    async throws -> Source
  {
    if let addXtreamFailure { throw addXtreamFailure }
    xtreamAdds.append(
      XtreamAdd(name: name, server: server, username: username, password: password))
    let id = nextId
    nextId += 1
    // `secretRef`, not the password: the core mints an opaque key and the credential goes to the
    // host secure store, so a Source record never carries one (TECH_SPEC §12).
    return .xtream(
      id: id, common: SourceCommon(name: name, enabled: true, autoRefreshSecs: nil),
      server: server, username: username, secretRef: "secret-\(id)")
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
