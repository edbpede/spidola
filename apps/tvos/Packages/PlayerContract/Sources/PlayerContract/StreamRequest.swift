// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

/// A stream header override applied at playback time. Token-bearing values arrive from the
/// host-secrets callback at play time and are never persisted here (TECH_SPEC §12).
public struct StreamHeader: Sendable, Equatable, Hashable {
  public let name: String
  public let value: String

  public init(name: String, value: String) {
    self.name = name
    self.value = value
  }
}

/// How much latency the viewer trades for resilience. Engine-neutral by construction: the
/// settings screen speaks this vocabulary, and each engine maps it onto its own knobs, so
/// settings language never names an engine (TECH_SPEC §8).
public enum BufferingProfile: String, Sendable, Equatable, Hashable, CaseIterable {
  /// Smallest buffer: fastest zap, least tolerant of a jittery source.
  case low
  /// The default trade-off.
  case balanced
  /// Largest buffer: slowest to start, rides out a bad connection.
  case generous

  /// The couch-legible label (PRD §8.6 voice) — describes the trade, not the buffer.
  public var label: String {
    switch self {
    case .low: "Fastest start"
    case .balanced: "Balanced"
    case .generous: "Smoothest playback"
    }
  }
}

/// Everything an engine needs to open a stream. Flat, owned, and engine-neutral: the same value
/// loads on any engine, which is what lets "Try other player" re-issue the identical request.
public struct StreamRequest: Sendable, Equatable, Hashable {
  /// The stream URL, already validated by the core's locator type.
  public let locator: String
  /// Per-channel/source header overrides.
  public let headers: [StreamHeader]
  /// Per-channel/source user-agent override; `nil` means the engine's own default.
  public let userAgent: String?
  /// The latency/resilience trade-off to apply.
  public let buffering: BufferingProfile

  public init(
    locator: String,
    headers: [StreamHeader] = [],
    userAgent: String? = nil,
    buffering: BufferingProfile = .balanced
  ) {
    self.locator = locator
    self.headers = headers
    self.userAgent = userAgent
    self.buffering = buffering
  }
}

/// How video fills the screen. Cycled by the playback UI; every engine honours the same set.
public enum AspectMode: String, Sendable, Equatable, Hashable, CaseIterable {
  /// Preserve aspect, letterbox to fit.
  case fit
  /// Preserve aspect, crop to fill.
  case fill
  /// Ignore aspect, stretch to the screen.
  case stretch

  /// The next mode in the cycle, so the UI's aspect button is one call with no index maths.
  public var next: AspectMode {
    switch self {
    case .fit: .fill
    case .fill: .stretch
    case .stretch: .fit
    }
  }

  /// The couch-legible label (PRD §8.6 voice).
  public var label: String {
    switch self {
    case .fit: "Fit"
    case .fill: "Fill"
    case .stretch: "Stretch"
    }
  }
}
