// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import Foundation
import Libmpv
import OSLog

/// A property the event loop asks mpv to report changes for.
struct MPVObservedProperty: Sendable {
  let name: String
  let format: mpv_format
}

/// mpv's blocking event loop, bridged into an `AsyncStream`.
///
/// **Why a raw `Thread`, in a codebase that otherwise forbids one:** `mpv_wait_event` is a blocking
/// C call with no async form and no callback that replaces it. It cannot be awaited, so it needs a
/// thread allowed to sit in it indefinitely. Running it on the cooperative pool (a `Task` or a
/// `@concurrent` function) would park a pool thread forever — the pool is sized to the core count,
/// so a few engines would starve it and deadlock unrelated work. A dedicated thread is the honest
/// primitive for "this call blocks and nothing can change that". This and `MPVCoreDisposal` are the
/// only threads `PlayerMPV` creates, and the `AsyncStream` boundary is the only seam here that is
/// not plain structured concurrency.
///
/// The ownership rule this upholds: the client handle is touched **only** by the loop's thread.
/// Nothing else — not the engine, not the main actor — ever calls into it. That is what makes
/// teardown provably free of use-after-free; see `MPVEngine.stop()`.
enum MPVEventLoop {
  /// Starts the loop on its own thread and returns the events it will produce.
  ///
  /// Takes ownership of `clientHandle`: the caller must not use it again. The thread destroys the
  /// handle when the core shuts down and then exits, so there is nothing to join and no object to
  /// keep — which is why this is a free function rather than a class the engine would have to hold
  /// and remember to tear down.
  static func start(
    clientHandle: OpaquePointer,
    observedProperties: [MPVObservedProperty]
  ) -> AsyncStream<MPVEvent> {
    // Unbounded: dropping mpv events would lose exactly the terminal `endFile` that decides whether
    // the viewer sees an error. The volume is a handful of events per load, not a stream that can
    // outrun its consumer.
    let (stream, continuation) = AsyncStream<MPVEvent>.makeStream(bufferingPolicy: .unbounded)

    let body = MPVEventLoopBody(
      clientAddress: Int(bitPattern: clientHandle),
      continuation: continuation,
      observedProperties: observedProperties)

    let thread = Thread { body.run() }
    thread.name = "dev.spidola.tv.mpv-events"
    // User-initiated, not default: this thread carries the frames the viewer is waiting on during a
    // zap, and a default-priority thread here shows up directly in the PRD §9 budget.
    thread.qualityOfService = .userInitiated
    thread.start()

    return stream
  }
}

/// The event thread's body.
///
/// Split out from the `start` function so the thread closure captures only this value — capturing
/// the engine's objects would keep its whole graph alive for as long as the thread ran.
///
/// **On `clientAddress` being an `Int`:** the handle crosses the thread boundary as its bit pattern
/// rather than as an `OpaquePointer` or an `MPVHandle`. Neither of those is `Sendable`, and making
/// one so would mean `@unchecked Sendable` — an assertion to the compiler that the handle is safe to
/// *share*, which is the opposite of true and would license exactly the aliasing this design exists
/// to prevent. An address genuinely is a number, `Int` genuinely is `Sendable`, and spelling the
/// transfer this way keeps the compiler's data-race proof intact while making the hand-off visible
/// to the reader. The wrapper is rebuilt on the far side, by the thread that owns it.
private struct MPVEventLoopBody: Sendable {
  let clientAddress: Int
  let continuation: AsyncStream<MPVEvent>.Continuation
  let observedProperties: [MPVObservedProperty]

  func run() {
    guard let raw = OpaquePointer(bitPattern: clientAddress) else {
      // Only reachable if a null address was handed over, which `createWeakClientHandle` already
      // rules out. Checked rather than force-unwrapped because a `!` here would crash the app to
      // report a condition the caller has already made impossible.
      continuation.finish()
      return
    }
    let client = MPVHandle(adopting: raw)

    guard setUp(client) else {
      // Setup failed, so nothing will drive the state machine and the engine would sit in
      // `.loading` forever — the one outcome worse than a reported failure. Emit shutdown so the
      // engine reaches a terminal state and the shell disposes it, then tear down on the same path
      // the normal exit uses.
      Logger.mpv.error("mpv event-loop setup failed; terminating engine")
      continuation.yield(.shutdown)
      finish(client)
      return
    }

    // `-1` blocks until an event arrives — no polling, no wakeup timer. The loop's only exit is
    // MPV_EVENT_SHUTDOWN, which the core sends once the engine destroys the last strong handle.
    while true {
      guard let raw = client.waitEvent(timeout: -1) else { continue }
      guard let event = MPVEvent.translate(raw) else { continue }

      continuation.yield(event)
      if case .shutdown = event { break }
    }

    finish(client)
  }

  /// Per-handle setup, run here because observed properties and the log-message level are
  /// *per-client* state in libmpv — registering them on the core handle would silently observe
  /// nothing on this one.
  private func setUp(_ client: MPVHandle) -> Bool {
    do {
      // mpv's log stream is requested at `warn` and read *only* to classify why a load failed
      // (`MPVErrorMapping.logHint`). Its text never reaches OSLog: mpv logs full stream URLs, and an
      // Xtream URL carries the account in its path (TECH_SPEC §12). FFmpeg reports HTTP status and
      // decode failures at warning level, so `error` would erase the distinctions the contract
      // needs; `warn` is still narrow enough to exclude resolved options and normal stream data.
      try client.requestLogMessages(level: "warn")
      for property in observedProperties {
        try client.observe(property.name, format: property.format)
      }
      return true
    } catch {
      return false
    }
  }

  /// Ordering matters, which is why it is one function rather than repeated at two call sites:
  ///  1. finish the stream, so the engine's consuming task ends rather than awaiting a dead core;
  ///  2. destroy the client handle, the last reference the core is waiting on.
  /// Nothing touches the handle after this returns, on any thread, ever.
  private func finish(_ client: MPVHandle) {
    continuation.finish()
    client.destroy()
  }
}
