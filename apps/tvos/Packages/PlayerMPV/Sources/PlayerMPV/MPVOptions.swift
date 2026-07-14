// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import PlayerContract

/// One mpv option as a name/value pair. A value type so the whole contract-to-mpv translation is a
/// pure function returning data, and the handle-touching code that applies it stays trivial.
struct MPVOption: Sendable, Equatable {
  let name: String
  let value: String

  init(_ name: String, _ value: String) {
    self.name = name
    self.value = value
  }
}

/// The contract's engine-neutral vocabulary translated into mpv's knobs (TECH_SPEC §8: settings
/// language never names an engine, so every engine owns its own mapping).
enum MPVOptions {

  // MARK: - Buffering

  /// mpv's cache options for a buffering profile.
  ///
  /// **On the numbers, honestly:** these are reasoned starting points, not measured ones. They are
  /// placed relative to mpv's own defaults (`cache-secs` 10, `demuxer-readahead-secs` 1) and
  /// pull in opposite directions on purpose — the profile exists to trade zap latency against
  /// resilience, so `.low` must actually start faster and `.generous` must actually ride out more
  /// jitter. Tuning them against the PRD §9 zap budget needs a real headend and a device, which is
  /// the engine acceptance suite's job, not a guess made here.
  ///
  /// `cache-pause-initial` is the lever that decides whether the first frame waits for a full
  /// buffer. It is off for `.low` and `.balanced` because "video visible within two seconds" is
  /// the zap budget, and on for `.generous`, where the viewer has explicitly asked for smoothness
  /// over speed.
  static func cacheOptions(for profile: BufferingProfile) -> [MPVOption] {
    switch profile {
    case .low:
      [
        MPVOption("cache", "yes"),
        MPVOption("cache-secs", "2"),
        MPVOption("demuxer-readahead-secs", "0.5"),
        MPVOption("cache-pause-initial", "no"),
      ]
    case .balanced:
      [
        MPVOption("cache", "yes"),
        MPVOption("cache-secs", "10"),
        MPVOption("demuxer-readahead-secs", "2"),
        MPVOption("cache-pause-initial", "no"),
      ]
    case .generous:
      [
        MPVOption("cache", "yes"),
        MPVOption("cache-secs", "30"),
        MPVOption("demuxer-readahead-secs", "10"),
        MPVOption("cache-pause-initial", "yes"),
      ]
    }
  }

  // MARK: - Aspect

  /// mpv's geometry options for an aspect mode.
  ///
  /// mpv expresses the three modes across two orthogonal knobs rather than one enum, so each mode
  /// must set both — leaving one unset would let the previous mode's value survive the cycle:
  ///
  /// - `keepaspect` — whether the source's aspect ratio is honoured at all.
  /// - `panscan` — how much of the letterbox is cropped away, `0` none through `1.0` full.
  ///
  /// `.fit` is keep-aspect with no crop (letterbox), `.fill` keeps aspect but crops to the frame,
  /// and `.stretch` abandons aspect entirely, at which point `panscan` has nothing to crop and is
  /// reset so a later `.fill` starts clean.
  static func aspectOptions(for mode: AspectMode) -> [MPVOption] {
    switch mode {
    case .fit:
      [MPVOption("keepaspect", "yes"), MPVOption("panscan", "0")]
    case .fill:
      [MPVOption("keepaspect", "yes"), MPVOption("panscan", "1.0")]
    case .stretch:
      [MPVOption("keepaspect", "no"), MPVOption("panscan", "0")]
    }
  }

  // MARK: - Headers

  /// Header overrides in the wire form mpv's `http-header-fields` list expects.
  ///
  /// Values are passed through verbatim — no escaping, no trimming. That is safe only because the
  /// caller sets this list as an `MPV_FORMAT_NODE` array (see `MPVHandle.setStringList`), which
  /// skips mpv's comma-splitting option parser. Rendering these into a comma-joined string would
  /// silently corrupt any value containing a comma, which is most `Accept` and `Cookie` headers.
  static func headerFields(_ headers: [StreamHeader]) -> [String] {
    headers.map { "\($0.name): \($0.value)" }
  }
}
