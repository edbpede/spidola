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
/// `Core`, hands feature code a narrow `CatalogAccess`, and bridges the import callback interface
/// into an `AsyncStream` whose termination cancels the core's task handle. UniFFI async methods
/// arrive back on the caller's continuation; callback events are trampolined to the caller's
/// isolation by the stream.
public final class SpidolaCore: CatalogAccess {
  private let core: Core

  public init(dbPath: String, logDirectives: String, secrets: SecretStore, logSink: LogSink) throws
  {
    core = try Core(
      config: CoreConfig(dbPath: dbPath, logDirectives: logDirectives),
      secrets: secrets,
      logSink: logSink
    )
  }

  /// The startup handshake (core / schema / boundary versions), checked before first use.
  public func handshake() -> Handshake { core.handshake() }

  public func sources() async throws -> [Source] { try await core.sources().list() }

  public func addM3uUrl(name: String, url: String) async throws -> Source {
    try await core.sources().addM3uUrl(
      name: name, url: url, userAgent: nil, acceptInvalidTls: false)
  }

  public func page(sourceId: Int64, offset: UInt32, limit: UInt32) async throws -> ChannelPage {
    try await core.catalog().channels(sourceId: sourceId, offset: offset, limit: limit)
  }

  /// Refreshes a source, emitting progress then a single terminal event. Cancelling the consuming
  /// task terminates the stream, which cancels the core task at the next batch boundary.
  public func importSource(id: Int64) -> AsyncStream<ImportEvent> {
    AsyncStream { continuation in
      let listener = ImportListenerAdapter(continuation: continuation)
      let handle = core.sources().refresh(id: id, listener: listener)
      continuation.onTermination = { _ in handle.cancel() }
    }
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
}
