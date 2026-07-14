// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import Foundation
import MediaPlayer

/// A transport request from the system — the Siri remote's play/pause, a voice command, the
/// system's playback UI.
enum MPVTransportCommand: Sendable, Equatable {
  case play
  case pause
  case togglePlayPause
  case seek(toSeconds: Double)
}

/// What the system should be told about what is playing.
///
/// A value type so the engine assembles the truth in one place and this file stays a translator.
/// `duration` and `position` are optional because a live stream genuinely has neither, and
/// inventing zeroes would draw a scrubber the viewer cannot use.
struct MPVNowPlayingState: Sendable, Equatable {
  var title: String?
  var duration: Double?
  var position: Double?
  var rate: Double
  var isLive: Bool
}

/// Now-playing reporting and system transport controls (TECH_SPEC §6: "honest now-playing-info
/// reporting" is an explicit engine acceptance point).
///
/// Honest is the operative word and the reason this is not a one-liner. The system's playback UI
/// renders whatever we publish, so publishing a duration for a live stream produces a scrubber that
/// lies, and leaving stale info behind after teardown leaves the viewer looking at a channel that
/// stopped playing minutes ago.
@MainActor
final class MPVNowPlaying {
  /// Transport requests from the system.
  let commands: AsyncStream<MPVTransportCommand>

  private let continuation: AsyncStream<MPVTransportCommand>.Continuation
  private var targets: [(command: MPRemoteCommand, token: Any)] = []
  private var isSeekable = false

  init() {
    let (stream, continuation) = AsyncStream<MPVTransportCommand>.makeStream(
      bufferingPolicy: .unbounded)
    self.commands = stream
    self.continuation = continuation
  }

  /// Registers for system transport commands.
  func activate() {
    let center = MPRemoteCommandCenter.shared()
    let continuation = self.continuation

    add(center.playCommand) { continuation.yield(.play) }
    add(center.pauseCommand) { continuation.yield(.pause) }
    add(center.togglePlayPauseCommand) { continuation.yield(.togglePlayPause) }

    let token = center.changePlaybackPositionCommand.addTarget { event in
      guard let event = event as? MPChangePlaybackPositionCommandEvent else {
        return .commandFailed
      }
      continuation.yield(.seek(toSeconds: event.positionTime))
      return .success
    }
    targets.append((center.changePlaybackPositionCommand, token))
  }

  /// Publishes the current state to the system.
  func update(_ state: MPVNowPlayingState) {
    var info: [String: Any] = [:]
    if let title = state.title { info[MPMediaItemPropertyTitle] = title }
    info[MPNowPlayingInfoPropertyPlaybackRate] = state.rate
    info[MPNowPlayingInfoPropertyIsLiveStream] = state.isLive

    // A live stream reports neither duration nor position: the system draws a scrubber from these,
    // and a scrubber on something unseekable is a control that does nothing. This is the honesty
    // the spec asks for, expressed as an omission rather than a zero.
    if !state.isLive {
      if let duration = state.duration { info[MPMediaItemPropertyPlaybackDuration] = duration }
      if let position = state.position {
        info[MPNowPlayingInfoPropertyElapsedPlaybackTime] = position
      }
    }

    MPNowPlayingInfoCenter.default().nowPlayingInfo = info

    // Seekability can only be known once the stream is open, so the scrubbing command's
    // availability is refreshed here rather than pinned at activation.
    if isSeekable != !state.isLive {
      isSeekable = !state.isLive
      MPRemoteCommandCenter.shared().changePlaybackPositionCommand.isEnabled = isSeekable
    }
  }

  /// Removes the targets and clears the system's now-playing info. Idempotent.
  ///
  /// Clearing matters: the info center is process-wide, so info left behind outlives the engine
  /// that published it and the system keeps showing a channel that is no longer playing.
  func deactivate() {
    for entry in targets {
      entry.command.removeTarget(entry.token)
    }
    targets.removeAll()
    MPNowPlayingInfoCenter.default().nowPlayingInfo = nil
    continuation.finish()
  }

  private func add(_ command: MPRemoteCommand, handler: @escaping @Sendable () -> Void) {
    let token = command.addTarget { _ in
      handler()
      return .success
    }
    targets.append((command, token))
  }
}
