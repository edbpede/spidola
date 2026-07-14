// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import CoreKit
import OSLog
import Observation
import core_api

/// Which add-source flow the form is driving. tvOS has no document picker, so a local playlist is
/// added by pasting its text; the URL flow fetches and streams it (TECH_SPEC §4.5). Xtream and
/// pairing land in Phase 6.
public enum AddSourceMode: Sendable, CaseIterable {
  case url
  case file

  public var title: String {
    switch self {
    case .url: "Playlist URL"
    case .file: "Paste a playlist"
    }
  }
}

/// The add-source screen's phase. A closed set the view matches exhaustively; `importing` carries
/// live progress, `done` the diagnostics summary, and `failed` a fully-formed `ActionableError`.
public enum AddSourceState: Sendable {
  case editing
  case importing(stage: ImportStage, channels: UInt64)
  case done(ImportOutcome)
  case failed(ActionableError)
}

/// Drives adding a source and importing its catalog with live progress, cancellation, and a
/// diagnostics summary (PRD §6.1). Depends on the narrow `SourcesAccess`, so it is unit-tested
/// against a fake. A cancelled or failed first import deletes the just-created empty source, so a
/// half-added source never litters the list.
@MainActor
@Observable
public final class AddSourceModel {
  public var mode: AddSourceMode = .url
  public var name = ""
  public var url = ""
  public var pastedContent = ""
  public var userAgent = ""
  public var acceptInvalidTls = false
  public private(set) var validationMessage: String?
  public private(set) var state: AddSourceState = .editing

  private let access: any SourcesAccess
  private var importTask: Task<Void, Never>?
  private let logger = Logger(subsystem: "dev.spidola.tv", category: "spidola::sources")

  /// Called once an import completes successfully, so the composition root can pop back to a
  /// refreshed sources list.
  public var onFinished: (@MainActor () -> Void)?

  public init(access: any SourcesAccess) {
    self.access = access
  }

  public var isImporting: Bool {
    if case .importing = state { return true }
    return false
  }

  public func submit() {
    guard validate() else { return }
    validationMessage = nil
    state = .importing(stage: .connecting, channels: 0)
    let mode = mode
    importTask = Task { [weak self] in await self?.run(mode: mode) }
  }

  /// Cancels the running import; the stream terminates, cancelling the core task at its next batch
  /// boundary, and the just-created source is removed.
  public func cancel() {
    importTask?.cancel()
  }

  /// Awaits the in-flight import task. Test-only seam (the import runs in a detached child task);
  /// production code observes `state` instead.
  func waitForImport() async {
    await importTask?.value
  }

  private func validate() -> Bool {
    let trimmedName = name.trimmingCharacters(in: .whitespacesAndNewlines)
    if trimmedName.isEmpty {
      validationMessage = "Give this source a name."
      return false
    }
    switch mode {
    case .url:
      if url.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
        validationMessage = "Enter the playlist address."
        return false
      }
    case .file:
      if pastedContent.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
        validationMessage = "Paste the playlist text."
        return false
      }
    }
    return true
  }

  private func run(mode: AddSourceMode) async {
    let created: Source
    do {
      created = try await createSource(mode: mode)
    } catch is CancellationError {
      state = .editing
      return
    } catch let error as ApiError {
      state = .failed(ActionableError(error))
      return
    } catch {
      state = .failed(ActionableError(.Internal))
      return
    }

    let stream: AsyncStream<ImportEvent>
    switch mode {
    case .url: stream = access.importURL(id: created.id)
    case .file: stream = access.importContent(id: created.id, content: pastedContent)
    }

    var imported = false
    var failure: ActionableError?
    loop: for await event in stream {
      switch event {
      case .progress(let progress):
        state = .importing(stage: progress.stage, channels: progress.channelsSeen)
      case .complete(let outcome):
        state = .done(outcome)
        imported = true
        break loop
      case .failed(.Cancelled):
        break loop
      case .failed(let error):
        failure = ActionableError(error)
        break loop
      }
    }

    if !imported {
      // Cancelled or failed: only a completed import earns the source its row, so drop the empty
      // one we just created. Settling the state after the delete keeps a fast retry from racing
      // the cleanup.
      await deleteQuietly(created.id)
      if let failure {
        state = .failed(failure)
      } else {
        state = .editing
      }
    }
  }

  private func createSource(mode: AddSourceMode) async throws -> Source {
    let trimmedName = name.trimmingCharacters(in: .whitespacesAndNewlines)
    switch mode {
    case .url:
      let agent = userAgent.trimmingCharacters(in: .whitespacesAndNewlines)
      return try await access.addM3uUrl(
        name: trimmedName,
        url: url.trimmingCharacters(in: .whitespacesAndNewlines),
        userAgent: agent.isEmpty ? nil : agent,
        acceptInvalidTls: acceptInvalidTls)
    case .file:
      return try await access.addM3uFile(name: trimmedName)
    }
  }

  private func deleteQuietly(_ id: Int64) async {
    do {
      try await access.deleteSource(id: id)
    } catch {
      logger.error(
        "cleanup of abandoned source failed: \(String(describing: error), privacy: .public)")
    }
  }
}
