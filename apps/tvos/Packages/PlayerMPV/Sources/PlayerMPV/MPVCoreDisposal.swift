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
  /// Takes ownership of `handle` and destroys it, keeping `renderTarget` alive until it is gone.
  /// The caller must not touch the pointer again.
  ///
  /// **Why the layer comes along.** mpv holds its `wid` render target as a bare address and never
  /// retains it (`MPVHandle.setWindowID`), so the layer must outlive the core — and this thread is
  /// exactly what makes that hard to arrange. It keeps the core alive past the caller's return,
  /// while the caller is typically releasing its own last reference to the layer at that same
  /// moment: `MPVEngine`'s `deinit` calls `stop()` and then, having nothing left to do, lets the
  /// layer go. `mpv_destroy` tears down the Vulkan swapchain built on that layer, so a reference has
  /// to be held here — this thread is the only place that knows when the core is actually gone.
  ///
  /// Both values cross as bit patterns: neither `OpaquePointer` nor `CAMetalLayer` is `Sendable`,
  /// and conforming either would be `@unchecked Sendable` — a claim that they are safe to *share*,
  /// which is precisely what these hand-offs must not be. An `Int` is honestly `Sendable`, and the
  /// safety argument rests where it belongs: on the caller having given up the pointer, and on the
  /// retain below being balanced by exactly one release.
  ///
  /// **Why the release goes back to the main actor.** This retain is normally the *last* one, not a
  /// spare. On the zap and exit paths `PlaybackModel` drops the engine and SwiftUI tears the hosting
  /// view down within the same turn — taking the view tree's own references to the layer
  /// (`MPVMetalSurface`, `MPVSurfaceView`) with it, on the main actor — while `mpv_destroy` is still
  /// blocking here, that being the whole reason it is here. So the
  /// release below is normally what deallocates the layer, and left on this thread it would run
  /// `CAMetalLayer.dealloc` at utility QoS, where before it always ran on the main actor. Core
  /// Animation's affinity inside its own teardown is not a thing to discover on a viewer's device,
  /// so the layer is handed back to the main actor to die there. The hop costs nothing that matters:
  /// it is enqueued only once `mpv_destroy` has returned, so no main-actor work waits on that call.
  static func dispose(_ handle: OpaquePointer, keepingAlive renderTarget: MPVMetalLayer) {
    let address = Int(bitPattern: handle)
    let targetAddress = Int(bitPattern: Unmanaged.passRetained(renderTarget).toOpaque())
    let thread = Thread {
      // Paired with `passRetained` above, on every exit from this closure, and on the main actor
      // for the reason given above.
      defer {
        Task { @MainActor in
          if let target = UnsafeRawPointer(bitPattern: targetAddress) {
            Unmanaged<MPVMetalLayer>.fromOpaque(target).release()
          }
        }
      }
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
