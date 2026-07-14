// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import Foundation

/// Bridges a KVO-observable property into an `AsyncStream`.
///
/// AVFoundation publishes the signals this engine's state machine is built from — item status,
/// buffer health, time-control status — exclusively through KVO. There is no async sequence, no
/// `Observable` conformance, and no delegate for any of them in the SDK, so the bridge has to
/// exist. Confining it to this one function is what keeps the rest of the engine written in the
/// idiom the Swift guidelines prescribe: `AVPlayerEngine` consumes everything with `for await`
/// inside a structured task, and no observation token is stored, invalidated, or forgotten
/// anywhere else.
///
/// This is the same shape the guidelines' own callback-bridging example uses — an `AsyncStream`
/// whose `onTermination` tears the underlying observation down — applied to KVO instead of a
/// delegate. Cancelling the consuming task ends iteration, which terminates the stream, which
/// invalidates the observation: one lifetime, not two.
///
/// `.initial` is included so a consumer that subscribes after the property already changed still
/// sees its current value. Without it, an item that reached `.readyToPlay` before the observing
/// task started would strand the engine in `.loading` forever.
func keyValueStream<Root: NSObject, Value: Sendable>(
  _ root: Root,
  _ keyPath: KeyPath<Root, Value> & Sendable
) -> AsyncStream<Value> {
  // Unbounded so a burst of transitions during start-up is delivered whole. These properties
  // change a handful of times per stream, so there is no firehose to bound against, and dropping
  // the one transition that mattered is the failure worth avoiding.
  AsyncStream(bufferingPolicy: .unbounded) { continuation in
    let observation = root.observe(keyPath, options: [.initial, .new]) { _, change in
      guard let value = change.newValue else { return }
      continuation.yield(value)
    }
    continuation.onTermination = { _ in observation.invalidate() }
  }
}
