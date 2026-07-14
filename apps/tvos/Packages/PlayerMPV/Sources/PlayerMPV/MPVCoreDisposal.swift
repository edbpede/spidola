// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import Foundation
import Libmpv

/// Destroys an mpv core off the main actor.
///
/// **Why this is not just a call to `mpv_destroy`.** Destroying the last strong handle blocks until
/// every weak client has answered the resulting `MPV_EVENT_SHUTDOWN` and the core has finished
/// uninitialising — releasing the decoder, the VideoToolbox session, and the Vulkan swapchain. On
/// the zap path that is main-thread time the PRD §9 budget does not have ("UI never blocked"), so
/// the wait happens somewhere else.
///
/// **Why a `Thread` rather than a `Task`.** The wait is a blocking C call, not a suspension. A
/// `@concurrent` function would park a cooperative-pool thread for the duration, and the pool is
/// sized to the core count — a viewer zapping quickly could park several at once and stall
/// unrelated work. This is the same reasoning that justifies the event thread in `MPVEventLoop`,
/// and these two are the only threads `PlayerMPV` creates.
///
/// The thread is fire-and-forget by design: nothing waits on it, and it exits as soon as the core
/// is gone. The teardown ordering it participates in is documented at `MPVEngine.stop()`.
enum MPVCoreDisposal {
  /// Takes ownership of `handle` and destroys it. The caller must not touch the pointer again.
  ///
  /// The handle crosses to the thread as its bit pattern: `OpaquePointer` is not `Sendable`, and
  /// conforming it would be `@unchecked Sendable` — a claim that the handle is safe to *share*,
  /// which is precisely what this hand-off must not be. An `Int` is honestly `Sendable`, and the
  /// safety argument rests where it belongs: on the caller having given up the pointer.
  static func dispose(_ handle: OpaquePointer) {
    let address = Int(bitPattern: handle)
    let thread = Thread {
      guard let handle = OpaquePointer(bitPattern: address) else { return }
      // The event loop's weak client is woken by this call, destroys itself, and lets the core
      // finish. Both handles die here, in that order, with no thread ever touching the other's.
      mpv_destroy(handle)
    }
    thread.name = "dev.spidola.tv.mpv-disposal"
    // Utility, not user-initiated: the viewer is already watching the *next* channel by the time
    // this runs, so it must not compete with the engine that is loading it.
    thread.qualityOfService = .utility
    thread.start()
  }
}
