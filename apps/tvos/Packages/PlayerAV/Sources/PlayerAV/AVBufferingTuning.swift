// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import Foundation
import PlayerContract

/// The AVPlayer knobs a `BufferingProfile` resolves to.
///
/// A value rather than two lines inside `load`, so the latency/resilience trade the settings
/// screen sells (TECH_SPEC §8: settings language stays engine-neutral) can be asserted without a
/// player, an asset, or a network.
struct AVBufferingTuning: Equatable {
  /// Seconds of media to buffer ahead, for `AVPlayerItem.preferredForwardBufferDuration`. Zero
  /// is that property's documented "you decide" value, not an absence.
  let forwardBufferDuration: TimeInterval

  /// For `AVPlayer.automaticallyWaitsToMinimizeStalling`: whether the player may hold the first
  /// frame back until it predicts an uninterrupted run.
  let waitsToMinimizeStalling: Bool

  init(_ profile: BufferingProfile) {
    switch profile {
    case .low:
      // Zapping is the interaction this profile exists for: the viewer is flipping channels and
      // may abandon this one within the second, so start on what has arrived and accept that a
      // jittery source will rebuffer. `waits = false` is the half that earns the profile — left
      // on, AVPlayer delays the first frame until it predicts a stall-free run, which is exactly
      // the delay PRD §9's two-second click-to-first-frame bar cannot pay for.
      self.forwardBufferDuration = 1
      self.waitsToMinimizeStalling = false
    case .balanced:
      // Zero hands the size to AVFoundation, which picks it from the HLS variant's declared
      // bandwidth and the throughput it is actually measuring. A hand-picked number here would
      // be a guess against data we do not have, and would be wrong in opposite directions for
      // the 480p and 4K variants of one channel.
      self.forwardBufferDuration = 0
      self.waitsToMinimizeStalling = true
    case .generous:
      // The "my connection drops out" answer: roughly ten seconds of runway, so a dropout passes
      // under the picture instead of stopping it, paid for at start-up.
      self.forwardBufferDuration = 10
      self.waitsToMinimizeStalling = true
    }
  }
}
