// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import CoreKit
import Observation
import core_api

/// An auto-refresh interval offered in the sources UI (PRD §6.1 per-source auto-refresh).
public enum AutoRefreshOption: Sendable, CaseIterable, Identifiable {
  case off
  case hourly
  case sixHourly
  case daily

  public var id: Self { self }

  public var seconds: UInt32? {
    switch self {
    case .off: nil
    case .hourly: 3600
    case .sixHourly: 6 * 3600
    case .daily: 24 * 3600
    }
  }

  public var label: String {
    switch self {
    case .off: String(localized: "Manual only", bundle: .module)
    case .hourly: String(localized: "Every hour", bundle: .module)
    case .sixHourly: String(localized: "Every 6 hours", bundle: .module)
    case .daily: String(localized: "Every day", bundle: .module)
    }
  }

  public static func from(seconds: UInt32?) -> AutoRefreshOption {
    allCases.first { $0.seconds == seconds } ?? .off
  }
}

/// Backs the manage-sources screen: the list plus rename / enable-disable / refresh / delete /
/// auto-refresh (PRD §6.1). Refresh preserves favorites and hidden flags via the core's stable
/// identity (§4.4), so the shell need do nothing special. Depends on the narrow `SourcesAccess`.
@MainActor
@Observable
public final class SourcesModel {
  public private(set) var state: LoadState<[Source]> = .loading
  public private(set) var refreshingIds: Set<Int64> = []
  public private(set) var statusMessage: String?

  private let access: any SourcesAccess

  public init(access: any SourcesAccess) {
    self.access = access
  }

  public func load() async {
    if case .ready = state {} else { state = .loading }
    do {
      let sources = try await access.sources()
      state = sources.isEmpty ? .empty : .ready(sources)
    } catch {
      if let failed = LoadState<[Source]>.failure(from: error) { state = failed }
    }
  }

  public func rename(id: Int64, to name: String) async {
    let trimmed = name.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmed.isEmpty else { return }
    await mutate { try await self.access.rename(id: id, name: trimmed) }
  }

  public func setEnabled(id: Int64, enabled: Bool) async {
    await mutate { try await self.access.setEnabled(id: id, enabled: enabled) }
  }

  public func setAutoRefresh(id: Int64, option: AutoRefreshOption) async {
    await mutate { try await self.access.setAutoRefresh(id: id, secs: option.seconds) }
  }

  public func delete(id: Int64) async {
    await mutate { try await self.access.deleteSource(id: id) }
  }

  public func refresh(_ source: Source) async {
    guard source.isRefreshable else {
      statusMessage = "This source was added from a file — re-add it to update its channels."
      return
    }
    refreshingIds.insert(source.id)
    defer { refreshingIds.remove(source.id) }
    for await event in access.importURL(id: source.id) {
      switch event {
      case .progress:
        continue
      case .complete(let outcome):
        statusMessage = "Refreshed \(source.name): \(outcome.inserted) channels"
      case .failed(.Cancelled):
        break
      case .failed(let error):
        statusMessage = ActionableError(error).message
      }
    }
    await load()
  }

  /// Runs a mutating action, surfacing any failure as a status message and reloading the list on
  /// success — so the UI always reflects the core, the single source of truth.
  private func mutate(_ action: @escaping () async throws -> Void) async {
    do {
      try await action()
      statusMessage = nil
      await load()
    } catch is CancellationError {
    } catch let error as ApiError {
      statusMessage = ActionableError(error).message
    } catch {
      statusMessage = ActionableError(.Internal).message
    }
  }
}
