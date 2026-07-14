// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import Libmpv

/// An observed mpv property's new value.
///
/// Only the shapes the engine observes are modelled. A wider "any mpv value" type would be a junk
/// drawer: every extra arm would be an untested arm, and mpv's node tree does not need
/// representing here because the one node-typed property we read (`track-list`) is fetched as JSON.
enum MPVPropertyValue: Sendable, Equatable {
  case flag(Bool)
  case double(Double)
  case string(String)
  /// The property changed but carries no value we read — a bare notification to go and fetch.
  case changed
}

/// One mpv event, as a value.
///
/// **Why a value type and not the C struct:** `mpv_wait_event` documents that its returned struct
/// and every pointer inside it are freed on the *next* `mpv_wait_event` call. Handing that struct
/// to another thread would be a use-after-free the moment the loop went round again. Translating to
/// an owned, `Sendable` Swift value on the event thread — copying every C string while it is still
/// alive — is what makes the hand-off to the main actor safe by construction rather than by timing.
enum MPVEvent: Sendable, Equatable {
  /// The core is going away. The event thread's cue to destroy its handle and exit.
  case shutdown
  /// The container is open and tracks are known.
  case fileLoaded
  /// Playback of the current file ended, for `reason`. `mpvError` is meaningful only when the
  /// reason is an error.
  case endFile(reason: mpv_end_file_reason, mpvError: Int32)
  /// An observed property changed.
  case propertyChanged(name: String, value: MPVPropertyValue)
  /// A log line from mpv or FFmpeg.
  ///
  /// The text is carried **only** so `MPVErrorMapping.logHint` can classify why a load failed; it
  /// is never forwarded to the log stream. mpv logs the full stream URL on open, and an Xtream URL
  /// carries the account in its path, so echoing this text to OSLog would leak credentials
  /// wholesale (TECH_SPEC §12). `MPVEngine` reads it, derives an `EngineError`, and drops it.
  case logMessage(level: mpv_log_level, text: String)
}

extension MPVEvent {
  /// Translates a live `mpv_event` into an owned value, or `nil` for events the engine ignores.
  ///
  /// Must be called on the thread that owns the handle, before the next `mpv_wait_event` — every
  /// pointer read here dies at that call. Every dereference below is guarded rather than forced:
  /// mpv sets `data` to null for events that carry none, and a `!` here would crash the app on a
  /// malformed stream instead of reporting it.
  static func translate(_ event: UnsafeMutablePointer<mpv_event>) -> MPVEvent? {
    switch event.pointee.event_id {
    case MPV_EVENT_SHUTDOWN:
      return .shutdown

    case MPV_EVENT_FILE_LOADED:
      return .fileLoaded

    case MPV_EVENT_END_FILE:
      guard let data = event.pointee.data else { return nil }
      let endFile = data.assumingMemoryBound(to: mpv_event_end_file.self).pointee
      return .endFile(reason: endFile.reason, mpvError: endFile.error)

    case MPV_EVENT_PROPERTY_CHANGE:
      guard let data = event.pointee.data else { return nil }
      let property = data.assumingMemoryBound(to: mpv_event_property.self).pointee
      guard let namePointer = property.name else { return nil }
      let name = String(cString: namePointer)
      return .propertyChanged(name: name, value: value(of: property))

    case MPV_EVENT_LOG_MESSAGE:
      guard let data = event.pointee.data else { return nil }
      let message = data.assumingMemoryBound(to: mpv_event_log_message.self).pointee
      guard let textPointer = message.text else { return nil }
      return .logMessage(level: message.log_level, text: String(cString: textPointer))

    default:
      return nil
    }
  }

  /// Reads an observed property's payload.
  ///
  /// `data` is null when mpv has no value for the property yet (it is unavailable, or this is the
  /// bare notification we asked for with `MPV_FORMAT_NONE`). That is a normal state, not an error,
  /// so it maps to `.changed` and the caller goes and fetches if it cares.
  private static func value(of property: mpv_event_property) -> MPVPropertyValue {
    guard let data = property.data else { return .changed }
    switch property.format {
    case MPV_FORMAT_FLAG:
      return .flag(data.assumingMemoryBound(to: Int32.self).pointee != 0)
    case MPV_FORMAT_DOUBLE:
      return .double(data.assumingMemoryBound(to: Double.self).pointee)
    case MPV_FORMAT_STRING:
      // A string property's data is a `char **`, not a `char *`.
      let indirect = data.assumingMemoryBound(to: UnsafePointer<CChar>?.self).pointee
      guard let indirect else { return .changed }
      return .string(String(cString: indirect))
    default:
      return .changed
    }
  }
}
