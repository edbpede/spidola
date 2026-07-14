// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import PlayerContract

/// What mpv currently says about itself, as the properties we observe report it.
///
/// mpv has no "state" property; playback state is an emergent fact spread across five independent
/// flags that arrive in any order. Collecting them into one value means the state machine is a
/// pure function of a struct instead of a scatter of `if` statements racing each other across
/// event callbacks.
struct MPVPlaybackFacts: Sendable, Equatable {
  /// `MPV_EVENT_FILE_LOADED` has arrived: the container is open and tracks are known.
  var fileLoaded = false
  /// mpv `paused-for-cache`: playback is starved, not stopped.
  var pausedForCache = false
  /// mpv `pause`: the viewer (or we) paused it.
  var paused = false
  /// mpv `core-idle`: the core is not producing frames, for any reason.
  var coreIdle = true
  /// mpv `eof-reached`: the stream ran out.
  var eofReached = false
}

/// The facts-to-state machine (TECH_SPEC §8).
enum MPVStateReducer {
  /// The contract state implied by `facts`.
  ///
  /// Order is the whole design here, so it is spelled out rather than left to the reader:
  ///
  /// 1. **eof** wins over everything — once the stream ran out, no other flag is interesting.
  /// 2. **not yet loaded** is `.loading`, which is the window the click-to-first-frame budget is
  ///    measured across (PRD §9). `core-idle` is also true here, so this arm must precede it or
  ///    every load would report `.buffering` before its first frame.
  /// 3. **paused** beats `paused-for-cache`: pausing sets both, and the viewer who pressed pause
  ///    should not be shown a spinner.
  /// 4. **paused-for-cache** is the real starve — `.buffering`.
  /// 5. **core-idle** without any of the above is still "no frames are coming", so it is
  ///    `.buffering` too, not `.playing`. Reporting `.playing` here is the classic mpv wiring bug:
  ///    the UI hides its spinner over a frozen frame.
  ///
  /// `.failed` is deliberately absent: failures arrive as `MPV_EVENT_END_FILE`, which carries a
  /// reason this struct cannot represent, and are mapped by `MPVErrorMapping`.
  static func state(from facts: MPVPlaybackFacts) -> PlaybackState {
    if facts.eofReached { return .ended }
    if !facts.fileLoaded { return .loading }
    if facts.paused { return .paused }
    if facts.pausedForCache { return .buffering }
    if facts.coreIdle { return .buffering }
    return .playing
  }
}
