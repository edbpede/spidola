// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import CoreKit
import Observation
import core_api

/// Backs the option-picker screen: the list of values one closed-set setting can take, with the
/// one in force marked.
///
/// It reads its own snapshot rather than being handed the root's. The picker is reached by a route,
/// and a route carries `Hashable` identifiers, not a settings record — and a value re-read at the
/// moment the picker opens is the honest one to mark anyway, since the core is the source of truth
/// and something else may have written it.
@MainActor
@Observable
public final class SettingsOptionsModel {
  public let field: SettingsField
  public private(set) var state: LoadState<AppSettings> = .loading

  private let access: any SettingsAccess

  public init(field: SettingsField, access: any SettingsAccess) {
    self.field = field
    self.access = access
  }

  /// The values on offer, in the order the picker shows them.
  public var choices: [SettingsChoice] { field.choices }

  /// The choice to mark as current, or `nil` when the stored value is none of the ones offered —
  /// the picker then marks nothing rather than pointing at a value that is not in force.
  public var selectedChoiceId: String? {
    guard case .ready(let settings) = state else { return nil }
    return field.selectedChoiceId(in: settings)
  }

  public func load() async {
    if case .ready = state {} else { state = .loading }
    do {
      state = .ready(try await access.settingsSnapshot())
    } catch {
      if let failed = LoadState<AppSettings>.failure(from: error) { state = failed }
    }
  }

  /// Writes a choice through to the core.
  ///
  /// - Returns: `true` when it landed, which is the caller's cue to close the picker. A failure
  ///   leaves the picker open showing an actionable error, because popping back to a root that
  ///   still shows the old value would tell the viewer their change was saved when it was not.
  @discardableResult
  public func choose(_ choiceId: String) async -> Bool {
    do {
      try await field.apply(choiceId: choiceId, using: access)
      return true
    } catch {
      // A cancellation is the screen going away under us, not a failure to report.
      if let failed = LoadState<AppSettings>.failure(from: error) { state = failed }
      return false
    }
  }
}
