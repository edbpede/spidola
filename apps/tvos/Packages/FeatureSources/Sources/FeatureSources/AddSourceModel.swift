// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import CoreKit
import OSLog
import Observation
import core_api

/// Which add-source flow the form is driving. tvOS has no document picker, so a local playlist is
/// added by pasting its text; the URL flow fetches and streams it (TECH_SPEC §4.5).
public enum AddSourceMode: Sendable, CaseIterable {
  case url
  case file
  case xtream

  public var title: String {
    switch self {
    case .url: "Playlist URL"
    case .file: "Paste a playlist"
    case .xtream: "Xtream account"
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
  public var server = ""
  public var username = ""
  /// The Xtream account password, in flight to `addXtream` and the host secure store behind it.
  /// It lives here only as long as the form does: it is never logged, never persisted by the
  /// shell, and what reaches SQLite is an opaque key the core mints (TECH_SPEC §12).
  public var password = ""
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
    case .xtream:
      if server.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
        validationMessage = "Enter the server address."
        return false
      }
      if username.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
        validationMessage = "Enter the username."
        return false
      }
      // Not trimmed: leading and trailing spaces are legal in a password, and silently eating
      // them would reject a correct one with a message about the account being wrong.
      if password.isEmpty {
        validationMessage = "Enter the password."
        return false
      }
    }
    return true
  }

  /// Returns a failed form to its fields so the details can be corrected — the action behind
  /// `ErrorAction.fixInput`, which is what `Unauthorized` prescribes and therefore what a rejected
  /// Xtream password lands on. Without this the "Edit" button on that error would re-render the
  /// same error, and the most likely failure on this screen would have no way out but Back.
  public func returnToForm() {
    state = .editing
    validationMessage = nil
  }

  /// Fills the form from what a phone sent, for the person at the TV to confirm (PRD §6.1). It
  /// pre-fills and never submits: the confirmation *is* the point, since anything on the LAN could
  /// have posted this.
  ///
  /// A submission carries no name — the form needs one and typing it on a remote is the misery
  /// pairing exists to avoid — so the host of whatever was sent becomes the default, which is both
  /// recognizable and editable.
  public func prefill(from submission: PairingSubmission) {
    switch submission {
    case .m3uUrl(let url):
      mode = .url
      self.url = url
      name = Self.hostLabel(of: url) ?? "Playlist"
    case .xtream(let server, let username, let password):
      mode = .xtream
      self.server = server
      self.username = username
      self.password = password
      name = Self.hostLabel(of: server) ?? "Xtream account"
    @unknown default:
      // A newer core sent a kind this build cannot fill in. Leaving the form untouched is the
      // honest response: the screen still works by hand (TECH_SPEC §5).
      break
    }
  }

  /// The host of a URL, as a name a person would recognize on the sources list.
  private static func hostLabel(of text: String) -> String? {
    guard let host = URLComponents(string: text)?.host, !host.isEmpty else { return nil }
    return host
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
    // Xtream fetches its catalog over the network exactly as a playlist URL does — `addXtream`
    // only verifies and stores the account, so the import is the same refresh call.
    case .url, .xtream: stream = access.importURL(id: created.id)
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
    case .xtream:
      // The password goes straight through, untrimmed and unlogged. This call verifies the account
      // against the headend before storing it, so a wrong one throws `Unauthorized` here and lands
      // on the form as a sentence rather than surfacing as a mystery on the next refresh.
      return try await access.addXtream(
        name: trimmedName,
        server: server.trimmingCharacters(in: .whitespacesAndNewlines),
        username: username.trimmingCharacters(in: .whitespacesAndNewlines),
        password: password)
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
