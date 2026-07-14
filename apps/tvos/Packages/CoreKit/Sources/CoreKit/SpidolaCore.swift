// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import Foundation
import core_api

/// Reads the source list and a source's channel catalog one page at a time (paged by contract,
/// TECH_SPEC §5). A narrow surface so a view model can be tested against a fake instead of the
/// real core.
public protocol CatalogAccess: Sendable {
  func sources() async throws -> [Source]
  func page(sourceId: Int64, offset: UInt32, limit: UInt32) async throws -> ChannelPage
}

/// One event from a running import; the stream terminates on `.complete` or `.failed`.
public enum ImportEvent: Sendable {
  case progress(ImportProgress)
  case complete(ImportOutcome)
  case failed(ApiError)
}

/// The single Swift-side handle on the Rust core (TECH_SPEC §5, §6). It wraps the generated
/// `Core`, conforms to the narrow per-feature access protocols the vertical slices depend on, and
/// bridges the import callback interface into an `AsyncStream` whose termination cancels the
/// core's task handle. UniFFI async methods arrive back on the caller's continuation; callback
/// events are trampolined to the caller's isolation by the stream.
public final class SpidolaCore: CatalogAccess, SourcesAccess, BrowseAccess, SearchAccess,
  HomeAccess
{
  private let core: Core

  public init(
    dbPath: String, logDirectives: String, secrets: SecretStore, logSink: LogSink
  ) throws {
    core = try Core(
      config: CoreConfig(dbPath: dbPath, logDirectives: logDirectives),
      secrets: secrets,
      logSink: logSink
    )
  }

  /// The startup handshake (core / schema / boundary versions), checked before first use.
  public func handshake() -> Handshake { core.handshake() }

  // MARK: - Sources

  public func sources() async throws -> [Source] { try await core.sources().list() }

  public func addM3uUrl(
    name: String, url: String, userAgent: String?, acceptInvalidTls: Bool
  ) async throws -> Source {
    try await core.sources().addM3uUrl(
      name: name, url: url, userAgent: userAgent, acceptInvalidTls: acceptInvalidTls)
  }

  /// Convenience for the fixture seeder and simple add flows (no user-agent, platform TLS).
  public func addM3uUrl(name: String, url: String) async throws -> Source {
    try await addM3uUrl(name: name, url: url, userAgent: nil, acceptInvalidTls: false)
  }

  public func addM3uFile(name: String) async throws -> Source {
    try await core.sources().addM3uFile(name: name)
  }

  public func rename(id: Int64, name: String) async throws {
    try await core.sources().rename(id: id, name: name)
  }

  public func setEnabled(id: Int64, enabled: Bool) async throws {
    try await core.sources().setEnabled(id: id, enabled: enabled)
  }

  public func setAutoRefresh(id: Int64, secs: UInt32?) async throws {
    try await core.sources().setAutoRefresh(id: id, secs: secs)
  }

  public func deleteSource(id: Int64) async throws {
    try await core.sources().delete(id: id)
  }

  public func importURL(id: Int64) -> AsyncStream<ImportEvent> {
    importStream { [core] listener in core.sources().refresh(id: id, listener: listener) }
  }

  public func importContent(id: Int64, content: String) -> AsyncStream<ImportEvent> {
    importStream { [core] listener in
      core.sources().importM3uContent(id: id, content: content, listener: listener)
    }
  }

  /// Kept for the M0 fixture seeder, which imports an M3U-by-URL source. Same as `importURL`.
  public func importSource(id: Int64) -> AsyncStream<ImportEvent> { importURL(id: id) }

  /// Builds the import `AsyncStream` shared by URL and content imports: it registers a listener,
  /// starts the core task, and cancels that task if the consuming task terminates early.
  private func importStream(
    _ start: @escaping @Sendable (ImportListener) -> TaskHandle
  ) -> AsyncStream<ImportEvent> {
    AsyncStream { continuation in
      let listener = ImportListenerAdapter(continuation: continuation)
      let handle = start(listener)
      continuation.onTermination = { _ in handle.cancel() }
    }
  }

  // MARK: - Catalog / browse

  public func page(sourceId: Int64, offset: UInt32, limit: UInt32) async throws -> ChannelPage {
    try await core.catalog().channels(sourceId: sourceId, offset: offset, limit: limit)
  }

  public func kinds(sourceId: Int64) async throws -> [MediaKind] {
    try await core.catalog().kinds(sourceId: sourceId)
  }

  public func groups(
    sourceId: Int64, kind: MediaKind, offset: UInt32, limit: UInt32
  ) async throws -> BrowseGroupPage {
    try await core.catalog().groups(sourceId: sourceId, kind: kind, offset: offset, limit: limit)
  }

  public func channelsInGroup(
    sourceId: Int64, kind: MediaKind, group: String?, offset: UInt32, limit: UInt32
  ) async throws -> ChannelPage {
    try await core.catalog().channelsInGroup(
      sourceId: sourceId, kind: kind, group: group, offset: offset, limit: limit)
  }

  public func isHidden(sourceId: Int64, identity: Int64) async throws -> Bool {
    try await core.catalog().isHidden(sourceId: sourceId, identity: identity)
  }

  public func setHidden(sourceId: Int64, identity: Int64, hidden: Bool) async throws {
    try await core.catalog().setHidden(sourceId: sourceId, identity: identity, hidden: hidden)
  }

  // MARK: - Favorites

  public func isFavorite(sourceId: Int64, identity: Int64) async throws -> Bool {
    try await core.favorites().isFavorite(sourceId: sourceId, identity: identity)
  }

  public func setFavorite(sourceId: Int64, identity: Int64, favorite: Bool) async throws {
    if favorite {
      try await core.favorites().add(sourceId: sourceId, identity: identity)
    } else {
      try await core.favorites().remove(sourceId: sourceId, identity: identity)
    }
  }

  public func favoriteIdentities(sourceId: Int64) async throws -> [Int64] {
    try await core.favorites().list(sourceId: sourceId).map(\.identity)
  }

  public func favoriteChannels(offset: UInt32, limit: UInt32) async throws -> ChannelPage {
    try await core.favorites().favoriteChannels(offset: offset, limit: limit)
  }

  // MARK: - Recents

  public func recents(limit: UInt32) async throws -> [Recent] {
    try await core.recents().list(limit: limit)
  }

  public func recentsEnabled() async throws -> Bool {
    try await core.recents().isEnabled()
  }

  public func setRecentsEnabled(_ enabled: Bool) async throws {
    try await core.recents().setEnabled(enabled: enabled)
  }

  public func clearRecents() async throws {
    try await core.recents().clear()
  }

  public func recordRecent(_ channel: PlayableChannel) async throws {
    try await core.recents().record(
      sourceId: channel.sourceId,
      identity: channel.identity,
      name: channel.name,
      locator: channel.locator,
      positionSecs: nil)
  }

  // MARK: - Search

  public func search(
    query: String, sourceId: Int64?, kind: MediaKind?, offset: UInt32, limit: UInt32
  ) async throws -> SearchPage {
    try await core.search().search(
      query: query, sourceId: sourceId, kind: kind, offset: offset, limit: limit)
  }
}

/// Bridges the UniFFI `ImportListener` callback (which may arrive on any core thread) onto the
/// import `AsyncStream`. The continuation is `Sendable`, so no lock is needed.
private final class ImportListenerAdapter: ImportListener {
  private let continuation: AsyncStream<ImportEvent>.Continuation

  init(continuation: AsyncStream<ImportEvent>.Continuation) {
    self.continuation = continuation
  }

  func onProgress(progress: ImportProgress) {
    continuation.yield(.progress(progress))
  }

  func onComplete(outcome: ImportOutcome) {
    continuation.yield(.complete(outcome))
    continuation.finish()
  }

  func onFailed(error: ApiError) {
    continuation.yield(.failed(error))
    continuation.finish()
  }
}

extension Source {
  /// The stable rowid of a source, regardless of its kind. The `@unknown default` reserves the
  /// "unknown future variant" arm the FFI boundary rules require (TECH_SPEC §5).
  public var id: Int64 {
    switch self {
    case .m3uUrl(let id, _, _, _, _): id
    case .m3uFile(let id, _): id
    case .xtream(let id, _, _, _, _): id
    @unknown default: -1
    }
  }

  /// The user-facing source name shared by every source kind.
  public var name: String {
    switch self {
    case .m3uUrl(_, let common, _, _, _): common.name
    case .m3uFile(_, let common): common.name
    case .xtream(_, let common, _, _, _): common.name
    @unknown default: ""
    }
  }

  /// The common per-source settings (enabled, auto-refresh) shared by every kind.
  public var common: SourceCommon {
    switch self {
    case .m3uUrl(_, let common, _, _, _): common
    case .m3uFile(_, let common): common
    case .xtream(_, let common, _, _, _): common
    @unknown default: SourceCommon(name: "", enabled: true, autoRefreshSecs: nil)
    }
  }

  /// Whether this source can be refreshed from a URL (M3U-by-URL and Xtream). File sources are
  /// import-once and re-import from a freshly picked/pasted file instead.
  public var isRefreshable: Bool {
    switch self {
    case .m3uUrl: true
    case .xtream: true
    case .m3uFile: false
    @unknown default: false
    }
  }

  /// A couch-legible one-word description of the source kind, for the sources list.
  public var kindLabel: String {
    switch self {
    case .m3uUrl: "Playlist URL"
    case .m3uFile: "Playlist file"
    case .xtream: "Xtream account"
    @unknown default: "Source"
    }
  }
}
