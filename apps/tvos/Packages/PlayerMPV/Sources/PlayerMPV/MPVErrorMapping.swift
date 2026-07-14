// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import Libmpv
import PlayerContract

/// mpv's failure vocabulary translated into the contract's taxonomy (TECH_SPEC §8).
///
/// Pure by construction — every function here is a total function of its arguments, with no handle,
/// no I/O, and no clock. That is what lets the mapping be unit-tested branch by branch while the
/// decoder it describes cannot run on a build machine at all.
enum MPVErrorMapping {

  // MARK: - Log-line classification

  /// The failure a single mpv/FFmpeg log line implies, or `nil` if the line says nothing about why
  /// loading failed.
  ///
  /// This is a heuristic over human-readable strings, and it is worth being honest about why we are
  /// reduced to one: `MPV_ERROR_LOADING_FAILED` is mpv's single answer for "the URL did not open",
  /// covering DNS failure, a refused connection, an HTTP 401, and an unrecognised container alike.
  /// The distinction the PRD's error UX depends on — retry vs. fix your login vs. try the other
  /// player — exists only in the log text FFmpeg emitted on the way. So we read it.
  ///
  /// Matching is case-insensitive and substring-based rather than anchored, because these strings
  /// come from two upstreams (mpv's own messages and FFmpeg's protocol layer) and carry varying
  /// prefixes. A miss returns `nil` and degrades to the code-based mapping — never to a wrong
  /// class, which would send the viewer down the wrong recovery path.
  static func logHint(from text: String) -> EngineError? {
    let line = text.lowercased()

    // Auth first: an HTTP 401/403 is also reported as a generic "failed to open", so a later
    // unreachable-shaped match must not win over it.
    if line.contains("401") && line.contains("unauthorized") { return .unauthorized }
    if line.contains("403") && line.contains("forbidden") { return .unauthorized }
    if line.contains("server returned 401") || line.contains("server returned 403") {
      return .unauthorized
    }
    if line.contains("authorization failed") || line.contains("authentication failed") {
      return .unauthorized
    }

    // Reachability: name resolution, refused/unreachable peers, and HTTP 404/5xx all mean the same
    // thing to the viewer — the channel's server did not answer usefully — and no engine swap
    // would change that.
    if line.contains("failed to resolve") || line.contains("name or service not known") {
      return .sourceUnreachable
    }
    if line.contains("connection refused") || line.contains("connection reset") {
      return .sourceUnreachable
    }
    if line.contains("network is unreachable") || line.contains("no route to host") {
      return .sourceUnreachable
    }
    if line.contains("server returned 404") || line.contains("server returned 5") {
      return .sourceUnreachable
    }
    if line.contains("tcp_connect") || line.contains("connection timed out") {
      return .sourceUnreachable
    }

    // Demux: the bytes arrived but nothing could read them as a container.
    if line.contains("failed to recognize file format") { return .unsupportedFormat }
    if line.contains("could not determine file format") { return .unsupportedFormat }
    if line.contains("invalid data found when processing input") { return .unsupportedFormat }
    if line.contains("no such file or directory") { return .unsupportedFormat }

    // Decode: the container opened and a codec inside it failed.
    if line.contains("could not open codec") || line.contains("no decoder found") {
      return .decoderFailed
    }
    if line.contains("decoder init failed") || line.contains("could not initialize video chain") {
      return .decoderFailed
    }
    if line.contains("could not open video decoder")
      || line.contains("could not open audio decoder")
    {
      return .decoderFailed
    }

    return nil
  }

  // MARK: - Error-code mapping

  /// The failure an mpv error code implies, refined by `logHint` where the code alone is ambiguous.
  ///
  /// `logHint` is consulted only for `MPV_ERROR_LOADING_FAILED`. The other codes are unambiguous on
  /// their own, and letting a stray log line override them would make the mapping depend on which
  /// message happened to be logged last — the exact non-determinism this function exists to avoid.
  static func engineError(mpvError: Int32, logHint: EngineError? = nil) -> EngineError {
    switch mpvError {
    case MPV_ERROR_LOADING_FAILED.rawValue:
      // The disambiguation point. Absent a hint we say unreachable: for a stream URL that would
      // not open, "the server didn't answer" is both the likeliest cause and the honest one — it
      // offers retry rather than the "Try other player" button, which cannot help here and would
      // waste the viewer's time (EngineError.offersOtherPlayer).
      return logHint ?? .sourceUnreachable

    case MPV_ERROR_UNKNOWN_FORMAT.rawValue, MPV_ERROR_NOTHING_TO_PLAY.rawValue:
      // Nothing demuxable in what arrived — the one case where another engine genuinely might win.
      return .unsupportedFormat

    case MPV_ERROR_UNSUPPORTED.rawValue:
      return .unsupportedFormat

    case MPV_ERROR_AO_INIT_FAILED.rawValue, MPV_ERROR_VO_INIT_FAILED.rawValue:
      // The stream was fine; our own output chain would not start. Classed as a decode failure
      // because that is the class whose recovery ("try the other player") actually applies.
      return .decoderFailed

    case MPV_ERROR_NOMEM.rawValue:
      return .decoderFailed

    case MPV_ERROR_SUCCESS.rawValue:
      // Reached only if a caller asks to classify a non-failure. Not representable as "no error"
      // because the return type is non-optional by design: every call site here is already on a
      // failure path, so a success code is a bug in the caller, not a state to model.
      return .unknown(detail: "mpv reported success on a failure path")

    default:
      return .unknown(detail: describe(mpvError: mpvError))
    }
  }

  // MARK: - End-of-file mapping

  /// The contract state an mpv `MPV_EVENT_END_FILE` implies, or `nil` when the end-of-file is one
  /// we caused and the shell should not be told anything.
  ///
  /// `STOP` and `QUIT` are returned as `nil` deliberately: both are the fingerprints of our own
  /// teardown, and reporting them would race a `.failed` or `.ended` onto the stream while the
  /// engine is being disposed on a zap — turning every channel change into a spurious error.
  /// `REDIRECT` is likewise silent; mpv is about to load the real target and the state machine
  /// should stay in `.loading` across it.
  static func endFileOutcome(
    reason: mpv_end_file_reason,
    mpvError: Int32,
    logHint: EngineError? = nil
  ) -> PlaybackState? {
    switch reason {
    case MPV_END_FILE_REASON_EOF:
      return .ended
    case MPV_END_FILE_REASON_ERROR:
      return .failed(engineError(mpvError: mpvError, logHint: logHint))
    case MPV_END_FILE_REASON_STOP, MPV_END_FILE_REASON_QUIT, MPV_END_FILE_REASON_REDIRECT:
      return nil
    default:
      // mpv's reason list has grown before and may grow again; an unrecognised reason is not a
      // failure to report at the viewer.
      return nil
    }
  }

  // MARK: - Diagnostics

  /// A safe-by-construction description of an mpv error code.
  ///
  /// `mpv_error_string` returns a pointer into a static table compiled into libmpv — a fixed
  /// English phrase like "loading failed". It cannot contain stream data, so unlike mpv's log
  /// text it is safe to put in `EngineError.unknown(detail:)`, which reaches the log stream
  /// (TECH_SPEC §12).
  static func describe(mpvError: Int32) -> String {
    guard let cString = mpv_error_string(mpvError) else {
      return "mpv error \(mpvError)"
    }
    return "mpv: \(String(cString: cString)) (\(mpvError))"
  }
}
