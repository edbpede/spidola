// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import Foundation
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
    case .retry: String(localized: "Try again", bundle: .module)
    case .goBack: String(localized: "Go back", bundle: .module)
    case .fixInput: String(localized: "Edit", bundle: .module)
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
        String(localized: "Can't reach the source", bundle: .module),
        String(
          localized:
            "Spidola couldn't connect. Check the address and your network, then try again.",
          bundle: .module),
        primary: .retry, other: [.goBack])
    case .Timeout:
      self.init(
        String(localized: "The source is slow to respond", bundle: .module),
        String(
          localized: "The source didn't answer in time. It may be busy — try again in a moment.",
          bundle: .module),
        primary: .retry, other: [.goBack])
    case .Unauthorized:
      self.init(
        String(localized: "Login was rejected", bundle: .module),
        String(
          localized: "The source didn't accept these sign-in details. Edit them and try again.",
          bundle: .module),
        primary: .fixInput, other: [.goBack])
    case .NotFound:
      self.init(
        String(localized: "Not available anymore", bundle: .module),
        String(localized: "This isn't at the source any longer.", bundle: .module),
        primary: .goBack, other: [])
    case .InvalidInput(let field, let issue):
      self.init(
        String(localized: "That entry isn't valid", bundle: .module),
        Self.invalidInputMessage(field: field, issue: issue),
        primary: .fixInput, other: [.goBack])
    case .ParseFailed:
      self.init(
        String(localized: "No channels found", bundle: .module),
        String(
          localized:
            "Spidola reached the source but found no channels to add. Check the playlist and try again.",
          bundle: .module),
        primary: .retry, other: [.goBack])
    case .StorageCorrupt:
      self.init(
        String(localized: "Local storage problem", bundle: .module),
        String(
          localized: "Something went wrong saving to this device. Try again.", bundle: .module),
        primary: .retry, other: [.goBack])
    case .Cancelled:
      self.init(
        String(localized: "Cancelled", bundle: .module),
        String(localized: "That was cancelled.", bundle: .module),
        primary: .goBack, other: [])
    case .Internal:
      self.init(
        String(localized: "Something went wrong", bundle: .module),
        String(localized: "An unexpected problem came up. Try again.", bundle: .module),
        primary: .retry, other: [.goBack])
    @unknown default:
      self.init(
        String(localized: "Something went wrong", bundle: .module),
        String(localized: "An unexpected problem came up. Try again.", bundle: .module),
        primary: .retry, other: [.goBack])
    }
  }

  /// Platform copy for the structured validation boundary. No core-authored prose crosses into UI.
  private static func invalidInputMessage(field: InputField, issue: InputIssue) -> String {
    let fieldName: String =
      switch field {
      case .address: String(localized: "address", bundle: .module)
      case .server: String(localized: "server address", bundle: .module)
      case .name: String(localized: "name", bundle: .module)
      case .header: String(localized: "request detail", bundle: .module)
      case .logLevel: String(localized: "log level", bundle: .module)
      case .file: String(localized: "file", bundle: .module)
      case .source: String(localized: "source", bundle: .module)
      @unknown default: String(localized: "entry", bundle: .module)
      }

    let format: String =
      switch issue {
      case .empty: String(localized: "Enter a value for the %@.", bundle: .module)
      case .invalid: String(localized: "Check the %@ and try again.", bundle: .module)
      case .unsupported: String(localized: "That %@ isn't supported.", bundle: .module)
      case .unavailable: String(localized: "That %@ isn't available right now.", bundle: .module)
      @unknown default: String(localized: "Check the %@ and try again.", bundle: .module)
      }
    return String(format: format, locale: .current, fieldName)
  }
}
