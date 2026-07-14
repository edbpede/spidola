// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import core_api

/// A generic screen load-state shared by the vertical slices' view models — the shell-side
/// vocabulary for a core-backed screen. A closed set the views match exhaustively; the failure arm
/// carries a fully-formed `ActionableError` (PRD §6.3), so an error is never a bare string. It lives
/// in CoreKit beside `ActionableError` because every slice speaks it and features never depend
/// sideways on one another (doctrine §3.1).
public enum LoadState<Value: Sendable>: Sendable {
  case loading
  case empty
  case ready(Value)
  case failed(ActionableError)
}

extension LoadState {
  /// Maps a thrown error into a `.failed` state, or `nil` for a cancellation — which must never be
  /// shown as an error, since a departing screen cancels its own load (TECH_SPEC §5 threading
  /// contract). Keeps the catch ladder in the view models to a single line.
  public static func failure(from error: Error) -> LoadState? {
    if error is CancellationError { return nil }
    if let api = error as? ApiError { return .failed(ActionableError(api)) }
    return .failed(ActionableError(.Internal))
  }
}
