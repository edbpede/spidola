// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import Foundation
import OSLog
import PlayerContract

extension Logger {
  /// The mpv engine's log category (TECH_SPEC §4.8): one category per span target, so a single
  /// predicate shows the whole playback story across core and engine.
  static let mpv = Logger(subsystem: "dev.spidola.tv", category: "spidola::player::mpv")
}

/// What may be written about a stream, and what may not (TECH_SPEC §12: secret values never
/// appear in interpolated log messages).
///
/// This exists as its own unit because the rule is an invariant, not a habit: every value the
/// engine logs about a request goes through here, so "did we leak a token?" is answered by
/// reading one file rather than auditing every call site.
enum MPVRedaction {
  /// A locator reduced to scheme, host, and port — the parts that diagnose a failure without
  /// carrying a credential.
  ///
  /// Dropping the path is not paranoia. An Xtream locator embeds the account directly in it
  /// (`http://host:8080/live/USERNAME/PASSWORD/123.ts`), and token-bearing query strings are the
  /// norm for the rest, so path and query are exactly where the secrets are. Host and port stay:
  /// they are what make a `sourceUnreachable` report actionable, and they are not credentials.
  static func locatorSummary(_ locator: String) -> String {
    guard let components = URLComponents(string: locator), let scheme = components.scheme else {
      return "<unparsable locator>"
    }
    guard let host = components.host else { return "\(scheme)://<no host>" }
    if let port = components.port { return "\(scheme)://\(host):\(port)" }
    return "\(scheme)://\(host)"
  }

  /// Header names only. A header override exists precisely to carry a token, so its value is
  /// assumed secret with no exception (TECH_SPEC §12) — the names alone answer "which overrides
  /// were applied?", which is the only question the log needs to settle.
  static func headerNames(_ headers: [StreamHeader]) -> String {
    headers.isEmpty ? "<none>" : headers.map(\.name).joined(separator: ", ")
  }

  /// Whether a user-agent override was set — never which one. A UA override is a fingerprint a
  /// source hands out to identify an account, so it is treated as token-shaped.
  static func userAgentPresence(_ userAgent: String?) -> String {
    userAgent == nil ? "default" : "overridden"
  }
}
