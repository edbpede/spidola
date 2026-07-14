// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import Foundation
import Libmpv

/// A failed libmpv call, carrying the code so the caller can map it (`MPVErrorMapping`).
struct MPVCallError: Error, Equatable {
  let code: Int32
  /// What we were doing. A fixed literal from our own source — never stream data, so it is safe to
  /// log (TECH_SPEC §12).
  let operation: String
}

/// A checked wrapper over one `mpv_handle`.
///
/// **Why this type exists:** libmpv is a C API that reports failure in return codes and hands back
/// raw pointers. Left inline, every call site would need its own `guard`, and the tempting shortcut
/// is `!` — which turns a stream that merely failed to open into a crash. Funnelling every call
/// through here means the pointer is unwrapped once, the status is checked once, and the rest of
/// the engine speaks Swift errors.
///
/// **Threading:** this type is intentionally *not* `Sendable` and carries no lock. libmpv permits
/// concurrent calls on one handle from several threads, but the engine deliberately does not rely
/// on that: each handle has exactly one owning thread (`MPVEngine` owns the core handle on the main
/// actor; `MPVEventLoop` owns its own client handle on its own thread). Single ownership is what
/// makes the teardown ordering in `MPVEngine.stop()` provably free of use-after-free, and a
/// `Sendable` conformance here would quietly license the sharing that breaks it.
final class MPVHandle {
  /// The C handle. Non-optional: a `MPVHandle` that exists is always backed by a live mpv handle,
  /// and the lifetime is closed by `destroy()` consuming the wrapper.
  let raw: OpaquePointer

  /// Adopts an existing handle. Used by the thread taking ownership of a handle another thread
  /// created — the wrapper is always built by the thread that will own it.
  init(adopting raw: OpaquePointer) {
    self.raw = raw
  }

  /// Creates an uninitialised core handle. Options must be set before `initialize()`.
  static func create() throws -> MPVHandle {
    guard let raw = mpv_create() else {
      throw MPVCallError(code: MPV_ERROR_NOMEM.rawValue, operation: "mpv_create")
    }
    return MPVHandle(adopting: raw)
  }

  /// Creates a **weak** client handle sharing this core, returning the raw pointer for transfer to
  /// its owning thread.
  ///
  /// Weak is the load-bearing word. A weak client does not keep the core alive, and when the last
  /// strong handle is destroyed, every weak client is sent `MPV_EVENT_SHUTDOWN`. That is exactly
  /// the wake-up the event thread needs: it lets `MPVEngine.stop()` tear the core down without ever
  /// touching the event thread's handle, so no thread can free a handle another is inside. See
  /// `MPVEngine.stop()` for the full ordering.
  ///
  /// A raw pointer rather than an `MPVHandle` is returned on purpose: wrapping it here would leave
  /// a usable wrapper on the creating thread, and the invariant is that this handle has exactly one
  /// owner from birth.
  func createWeakClientHandle(name: String) throws -> OpaquePointer {
    guard let raw = mpv_create_weak_client(self.raw, name) else {
      throw MPVCallError(code: MPV_ERROR_GENERIC.rawValue, operation: "mpv_create_weak_client")
    }
    return raw
  }

  func initialize() throws {
    try check(mpv_initialize(raw), "mpv_initialize")
  }

  /// Detaches this handle. May block until the core has finished uninitialising when this is the
  /// last strong handle, so the caller decides which thread pays that cost — never the main actor.
  func destroy() {
    mpv_destroy(raw)
  }

  // MARK: - Options and properties

  func setOption(_ name: String, _ value: String) throws {
    try check(mpv_set_option_string(raw, name, value), "set option \(name)")
  }

  func setOption(_ option: MPVOption) throws {
    try setOption(option.name, option.value)
  }

  func setProperty(_ name: String, _ value: String) throws {
    try check(mpv_set_property_string(raw, name, value), "set property \(name)")
  }

  func setFlag(_ name: String, _ value: Bool) throws {
    var flag: Int32 = value ? 1 : 0
    try withUnsafeMutablePointer(to: &flag) { pointer in
      try check(mpv_set_property(raw, name, MPV_FORMAT_FLAG, pointer), "set flag \(name)")
    }
  }

  func flag(_ name: String) -> Bool {
    var value: Int32 = 0
    let status = withUnsafeMutablePointer(to: &value) { pointer in
      mpv_get_property(raw, name, MPV_FORMAT_FLAG, pointer)
    }
    return status >= 0 && value != 0
  }

  func double(_ name: String) -> Double? {
    var value = 0.0
    let status = withUnsafeMutablePointer(to: &value) { pointer in
      mpv_get_property(raw, name, MPV_FORMAT_DOUBLE, pointer)
    }
    return status >= 0 ? value : nil
  }

  /// A property read as a string. Node-typed properties (`track-list`) come back as JSON, which is
  /// what `MPVTrackList` parses.
  ///
  /// mpv allocates the returned string and hands ownership over, so it is copied into a Swift
  /// `String` and released with `mpv_free` before returning — the `defer` keeps that true on every
  /// exit path.
  func string(_ name: String) -> String? {
    guard let cString = mpv_get_property_string(raw, name) else { return nil }
    defer { mpv_free(cString) }
    return String(cString: cString)
  }

  /// Sets a window id (`wid`) option from a layer pointer.
  ///
  /// mpv reads `wid` as an `int64_t` and MPVKit's MoltenVK context casts it straight back to a
  /// `CAMetalLayer *` (an unretained bridge). So the value is a raw address, and the layer must
  /// outlive the core — `MPVEngine` holds the only strong reference for exactly that reason.
  func setWindowID(_ pointer: UnsafeMutableRawPointer) throws {
    var wid = Int64(Int(bitPattern: pointer))
    try withUnsafeMutablePointer(to: &wid) { widPointer in
      try check(mpv_set_option(raw, "wid", MPV_FORMAT_INT64, widPointer), "set wid")
    }
  }

  /// Sets a string-list option without going through mpv's option-string parser.
  ///
  /// mpv parses a list option's string form by splitting on commas, so `Accept: text/html,text/xml`
  /// would arrive as two malformed headers. Passing an `MPV_FORMAT_NODE` array hands mpv the
  /// elements already separated, so values are taken verbatim however they are punctuated.
  ///
  /// All pointer work is confined to this function. Each element's C string is owned by `strdup`
  /// and freed on exit; mpv copies the value during `mpv_set_option`, so nothing here outlives the
  /// call and no pointer escapes the scope.
  func setStringList(_ name: String, _ values: [String]) throws {
    guard !values.isEmpty else { return }

    var cStrings: [UnsafeMutablePointer<CChar>?] = []
    defer { for pointer in cStrings { free(pointer) } }
    for value in values {
      guard let copy = strdup(value) else {
        throw MPVCallError(code: MPV_ERROR_NOMEM.rawValue, operation: "strdup for \(name)")
      }
      cStrings.append(copy)
    }

    var nodes: [mpv_node] = cStrings.map { pointer in
      var node = mpv_node()
      node.format = MPV_FORMAT_STRING
      node.u.string = pointer
      return node
    }

    try nodes.withUnsafeMutableBufferPointer { buffer in
      var list = mpv_node_list()
      list.num = Int32(buffer.count)
      list.values = buffer.baseAddress
      list.keys = nil
      try withUnsafeMutablePointer(to: &list) { listPointer in
        var root = mpv_node()
        root.format = MPV_FORMAT_NODE_ARRAY
        root.u.list = listPointer
        try withUnsafeMutablePointer(to: &root) { rootPointer in
          try check(mpv_set_option(raw, name, MPV_FORMAT_NODE, rootPointer), "set list \(name)")
        }
      }
    }
  }

  // MARK: - Commands

  /// Runs an mpv command.
  ///
  /// mpv wants a NULL-terminated `char *` array. Each argument is duplicated so the array outlives
  /// the Swift `String`s' own storage for the duration of the call, and freed on the way out — mpv
  /// copies whatever it needs before returning.
  func command(_ arguments: [String]) throws {
    // Typed as `UnsafePointer` to match mpv's `const char **`, while `free` takes the mutating view
    // back — the allocation is still ours, only the call's view of it is const.
    var cArguments: [UnsafePointer<CChar>?] = []
    defer {
      for pointer in cArguments where pointer != nil {
        free(UnsafeMutableRawPointer(mutating: pointer))
      }
    }
    for argument in arguments {
      guard let copy = strdup(argument) else {
        throw MPVCallError(code: MPV_ERROR_NOMEM.rawValue, operation: "strdup for command")
      }
      cArguments.append(UnsafePointer(copy))
    }
    cArguments.append(nil)

    try cArguments.withUnsafeMutableBufferPointer { buffer in
      try check(mpv_command(raw, buffer.baseAddress), "command \(arguments.first ?? "?")")
    }
  }

  // MARK: - Events

  func requestLogMessages(level: String) throws {
    try check(mpv_request_log_messages(raw, level), "mpv_request_log_messages")
  }

  func observe(_ name: String, format: mpv_format) throws {
    try check(mpv_observe_property(raw, 0, name, format), "observe \(name)")
  }

  /// Blocks until the next event. Only ever called by the handle's owning thread — libmpv forbids
  /// two threads waiting on one handle.
  func waitEvent(timeout: Double) -> UnsafeMutablePointer<mpv_event>? {
    mpv_wait_event(raw, timeout)
  }

  private func check(_ status: Int32, _ operation: String) throws {
    guard status < 0 else { return }
    throw MPVCallError(code: status, operation: operation)
  }
}
