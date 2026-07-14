// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import AVFAudio
import Foundation
import OSLog

/// What the system did to our audio, reduced to what the engine acts on.
enum MPVAudioInterruption: Sendable, Equatable {
  /// Something took the audio route — Siri, a call handoff. Playback must pause.
  case began
  /// We have it back. `shouldResume` carries the system's opinion: it is false when the viewer
  /// dismissed the interruption in a way that implies they want silence, and honouring it is the
  /// difference between a considerate player and one that talks over the next thing.
  case ended(shouldResume: Bool)
  /// The route changed underneath us (HDMI switch, AirPlay). Not itself a reason to pause, but the
  /// engine re-reports now-playing state so the new route's UI is not stale.
  case routeChanged
}

/// The audio session for long-form playback (TECH_SPEC §6: interruption handling is an explicit
/// engine acceptance test).
///
/// Its own unit because the session is a process-wide singleton with its own lifecycle, and mixing
/// that into the engine would make "who deactivated the session?" unanswerable once two engines
/// exist — which is exactly the situation the loud-fallback path creates.
@MainActor
final class MPVAudioSession {
  /// System audio events, in the order they happened.
  let interruptions: AsyncStream<MPVAudioInterruption>

  private let continuation: AsyncStream<MPVAudioInterruption>.Continuation
  private var observers: [NSObjectProtocol] = []
  private var isActive = false

  init() {
    let (stream, continuation) = AsyncStream<MPVAudioInterruption>.makeStream(
      bufferingPolicy: .unbounded)
    self.interruptions = stream
    self.continuation = continuation
  }

  /// Configures and activates the session.
  ///
  /// `.playback` is the category that says "this is the point of the app": it keeps audio alive
  /// when the screen idles and takes the route rather than mixing under whatever else is playing —
  /// the right posture for a channel the viewer is watching, and the wrong one for a sound effect.
  /// `.moviePlayback` mode gets the system's long-form tuning (dialogue enhancement, the correct
  /// spatialisation) instead of the default's general-purpose one.
  func activate() {
    let session = AVAudioSession.sharedInstance()
    do {
      try session.setCategory(.playback, mode: .moviePlayback)
      try session.setActive(true)
      isActive = true
    } catch {
      // A session that will not activate does not stop mpv decoding; it means the audio may not be
      // routed. Reported rather than thrown, because failing the whole load over it would deny the
      // viewer a picture they could otherwise watch.
      Logger.mpv.error("audio session activation failed; playback continues without it")
    }
    observe()
  }

  /// Deactivates and stops observing. Idempotent — `MPVEngine.stop()` is, so everything it calls
  /// must be.
  func deactivate() {
    for observer in observers {
      NotificationCenter.default.removeObserver(observer)
    }
    observers.removeAll()
    continuation.finish()

    guard isActive else { return }
    isActive = false
    do {
      // `.notifyOthersOnDeactivation` is what lets whatever we interrupted resume. Without it a
      // paused podcast stays paused after the viewer leaves playback, which reads as our bug.
      try AVAudioSession.sharedInstance().setActive(
        false, options: .notifyOthersOnDeactivation)
    } catch {
      Logger.mpv.error("audio session deactivation failed")
    }
  }

  /// Bridges the notifications into the stream.
  ///
  /// The block-based observer with a `nil` queue is deliberate. AVFoundation posts these from its
  /// own thread, and the alternatives are worse: `NotificationCenter.notifications(named:)` would
  /// carry a non-`Sendable` `Notification` across an isolation boundary, and a queue argument would
  /// drag in a dispatch dependency the house rules exclude. Instead the block reads the payload it
  /// needs *on the posting thread* and yields a `Sendable` value; the `Notification` never escapes.
  /// The `AsyncStream` does the hand-off, and the engine consumes it back on the main actor.
  private func observe() {
    let center = NotificationCenter.default
    let continuation = self.continuation

    observers.append(
      center.addObserver(
        forName: AVAudioSession.interruptionNotification, object: nil, queue: nil
      ) { notification in
        guard let raw = notification.userInfo?[AVAudioSessionInterruptionTypeKey] as? UInt,
          let type = AVAudioSession.InterruptionType(rawValue: raw)
        else { return }

        switch type {
        case .began:
          continuation.yield(.began)
        case .ended:
          let optionsRaw =
            notification.userInfo?[AVAudioSessionInterruptionOptionKey] as? UInt ?? 0
          let options = AVAudioSession.InterruptionOptions(rawValue: optionsRaw)
          continuation.yield(.ended(shouldResume: options.contains(.shouldResume)))
        @unknown default:
          return
        }
      })

    observers.append(
      center.addObserver(
        forName: AVAudioSession.routeChangeNotification, object: nil, queue: nil
      ) { _ in
        continuation.yield(.routeChanged)
      })
  }
}
