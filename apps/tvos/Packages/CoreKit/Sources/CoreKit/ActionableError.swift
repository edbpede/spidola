// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import core_api

/// A prescribed user action for an error (PRD §6.3). The set is deliberately small; the shell
/// renders each as a focusable button.
public enum ErrorAction: Sendable, Hashable {
  /// Try the same operation again.
  case retry
  /// Return to the previous screen.
  case goBack
  /// Correct the input that caused the failure.
  case fixInput

  /// The couch-legible button label (PRD §8.6 voice).
  public var label: String {
    switch self {
    case .retry: "Try again"
    case .goBack: "Go back"
    case .fixInput: "Edit"
    }
  }
}

/// The plain-language presentation of an `ApiError`: a short failure class, a one-sentence
/// message, and a **non-empty** set of prescribed actions (PRD §6.3, mirroring the core's
/// `ApiError::ux` table in `crates/core-api/src/error.rs`). Diagnostic detail stays in the log
/// stream, never here (PRD §8.6).
///
/// "No action available" is unrepresentable: `primaryAction` is a single required value, so every
/// `ActionableError` carries at least one action by construction — a UI that renders one can never
/// be handed an actionless error.
public struct ActionableError: Sendable, Equatable {
  /// A short, couch-legible failure class.
  public let failureClass: String
  /// A one-sentence, jargon-free explanation of what happened.
  public let message: String
  /// The recommended action, always present.
  public let primaryAction: ErrorAction
  /// Further actions offered after the primary one.
  public let otherActions: [ErrorAction]

  /// Every offered action, primary first — always non-empty.
  public var actions: [ErrorAction] { [primaryAction] + otherActions }

  private init(
    _ failureClass: String,
    _ message: String,
    primary: ErrorAction,
    other: [ErrorAction]
  ) {
    self.failureClass = failureClass
    self.message = message
    self.primaryAction = primary
    self.otherActions = other
  }

  /// Maps a boundary `ApiError` onto its presentation. The `@unknown default` reserves the
  /// "unknown future variant" arm the FFI boundary rules require (TECH_SPEC §5), so an older shell
  /// still presents a newer core's error with an action rather than crashing.
  public init(_ error: ApiError) {
    switch error {
    case .NetworkUnreachable:
      self.init(
        "Can't reach the source",
        "Spidola couldn't connect. Check the address and your network, then try again.",
        primary: .retry, other: [.goBack])
    case .Timeout:
      self.init(
        "The source is slow to respond",
        "The source didn't answer in time. It may be busy — try again in a moment.",
        primary: .retry, other: [.goBack])
    case .Unauthorized:
      self.init(
        "Login was rejected",
        "The source didn't accept these sign-in details. Edit them and try again.",
        primary: .fixInput, other: [.goBack])
    case .NotFound:
      self.init(
        "Not available anymore",
        "This isn't at the source any longer.",
        primary: .goBack, other: [])
    case .InvalidInput(let reason):
      self.init(
        "That entry isn't valid",
        reason,
        primary: .fixInput, other: [.goBack])
    case .ParseFailed:
      self.init(
        "No channels found",
        "Spidola reached the source but found no channels to add. Check the playlist and try again.",
        primary: .retry, other: [.goBack])
    case .StorageCorrupt:
      self.init(
        "Local storage problem",
        "Something went wrong saving to this device. Try again.",
        primary: .retry, other: [.goBack])
    case .Cancelled:
      self.init(
        "Cancelled",
        "That was cancelled.",
        primary: .goBack, other: [])
    case .Internal:
      self.init(
        "Something went wrong",
        "An unexpected problem came up. Try again.",
        primary: .retry, other: [.goBack])
    @unknown default:
      self.init(
        "Something went wrong",
        "An unexpected problem came up. Try again.",
        primary: .retry, other: [.goBack])
    }
  }
}
