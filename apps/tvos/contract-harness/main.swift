// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

// Swift half of the FFI parity keel (TECH_SPEC §5, §10, Phase 2 exit criteria).
//
// This is a bare test harness — no UI — that drives the SAME fixture flow as the Rust
// contract test (`crates/core-api/tests/contract.rs`) and the Kotlin harness, against the SAME
// compiled `core-api` library, through the generated Swift bindings. It builds a `Core` with a
// host secret store and log sink, imports a fixture playlist over a local HTTP stub while
// receiving progress callbacks, cancels a second import mid-stream, and asserts every FFI error
// variant is representable on the Swift side. Any core panic aborts the process (a red build);
// any assertion failure exits non-zero. Built and run by the Apple CI lane
// (`tools/ci/build-contract-harness-tvos.sh`) against the host-arch library.
//
// The generated `core_api.swift` is compiled into this same module, so the boundary types are
// referenced directly. `@unchecked Sendable` appears only here, in throwaway harness plumbing
// (never in shipped shell code), to bridge the callback interfaces onto blocking semaphores.

import Foundation

let resolvedDiagnosticSecret = "ffi-diagnostic-secret"

#if canImport(Glibc)
    import Glibc
#else
    import Darwin
#endif

// MARK: - Assertions

func fail(_ message: String) -> Never {
    FileHandle.standardError.write(Data("HARNESS FAIL: \(message)\n".utf8))
    exit(1)
}

func check(_ condition: Bool, _ message: @autoclosure () -> String) {
    if !condition { fail(message()) }
}

// MARK: - Host callback fakes

final class MemorySecrets: SecretStore, @unchecked Sendable {
    private let lock = NSLock()
    private var store: [String: String] = [:]
    func get(key: String) throws -> String? {
        lock.lock(); defer { lock.unlock() }
        return store[key]
    }
    func set(key: String, value: String) throws {
        lock.lock(); store[key] = value; lock.unlock()
    }
    func delete(key: String) throws {
        lock.lock(); store[key] = nil; lock.unlock()
    }
}

final class RecordingSink: LogSink, @unchecked Sendable {
    private let lock = NSLock()
    private var records: [LogRecord] = []
    func log(record: LogRecord) {
        lock.lock(); records.append(record); lock.unlock()
    }
    var targets: [String] {
        lock.lock(); defer { lock.unlock() }
        return records.map(\.target)
    }
}

final class Collector: ImportListener, @unchecked Sendable {
    enum Terminal { case complete(ImportOutcome); case failed(ApiError) }
    private let lock = NSLock()
    private var progress = 0
    private var firedFirst = false
    private var terminal: Terminal?
    private let firstProgress = DispatchSemaphore(value: 0)
    private let done = DispatchSemaphore(value: 0)

    func onProgress(progress: ImportProgress) {
        lock.lock()
        self.progress += 1
        let first = !firedFirst
        firedFirst = true
        lock.unlock()
        if first { firstProgress.signal() }
    }
    func onComplete(outcome: ImportOutcome) {
        lock.lock(); terminal = .complete(outcome); lock.unlock()
        done.signal()
    }
    func onFailed(error: ApiError) {
        lock.lock(); terminal = .failed(error); lock.unlock()
        done.signal()
    }

    var progressCount: Int {
        lock.lock(); defer { lock.unlock() }
        return progress
    }
    func awaitFirstProgress() { _ = firstProgress.wait(timeout: .now() + 60) }
    func awaitTerminal() -> Terminal {
        _ = done.wait(timeout: .now() + 60)
        lock.lock(); defer { lock.unlock() }
        return terminal ?? .failed(.Internal)
    }
}

// MARK: - Local HTTP stub

/// Serves `body` once over HTTP/1.1 from an ephemeral `127.0.0.1` port, paced in `chunk`-byte
/// slices with `delayUs` microseconds between them so the import genuinely streams (and can be
/// cancelled mid-flight).
func startStub(body: [UInt8], chunk: Int, delayUs: UInt32) -> String {
    let fd = socket(AF_INET, SOCK_STREAM, 0)
    check(fd >= 0, "socket()")
    var yes: Int32 = 1
    setsockopt(fd, SOL_SOCKET, SO_REUSEADDR, &yes, socklen_t(MemoryLayout<Int32>.size))
    var addr = sockaddr_in()
    addr.sin_family = sa_family_t(AF_INET)
    addr.sin_port = 0
    addr.sin_addr.s_addr = inet_addr("127.0.0.1")
    let bound = withUnsafePointer(to: &addr) {
        $0.withMemoryRebound(to: sockaddr.self, capacity: 1) {
            bind(fd, $0, socklen_t(MemoryLayout<sockaddr_in>.size))
        }
    }
    check(bound == 0, "bind()")
    check(listen(fd, 8) == 0, "listen()")

    var actual = sockaddr_in()
    var len = socklen_t(MemoryLayout<sockaddr_in>.size)
    _ = withUnsafeMutablePointer(to: &actual) {
        $0.withMemoryRebound(to: sockaddr.self, capacity: 1) {
            getsockname(fd, $0, &len)
        }
    }
    let port = UInt16(bigEndian: actual.sin_port)

    Thread.detachNewThread {
        let client = accept(fd, nil, nil)
        if client < 0 { return }
        var request = [UInt8](repeating: 0, count: 2048)
        _ = recv(client, &request, request.count, 0)
        let header =
            "HTTP/1.1 200 OK\r\nContent-Type: application/x-mpegurl\r\nContent-Length: \(body.count)\r\nConnection: close\r\n\r\n"
        let headerBytes = Array(header.utf8)
        _ = headerBytes.withUnsafeBytes { send(client, $0.baseAddress, headerBytes.count, 0) }
        var offset = 0
        while offset < body.count {
            let end = min(offset + chunk, body.count)
            let slice = Array(body[offset..<end])
            _ = slice.withUnsafeBytes { send(client, $0.baseAddress, slice.count, 0) }
            offset = end
            if delayUs > 0 { usleep(delayUs) }
        }
        close(client)
        close(fd)
    }
    return "http://127.0.0.1:\(port)/playlist.m3u"
}

func playlist(_ count: Int) -> String {
  var out = "#EXTM3U\n"
  for i in 0..<count {
    out += "#EXTINF:-1 tvg-id=\"id\(i)\" group-title=\"News\",Channel \(i)\n"
    if i == 0 {
      out += "#EXTVLCOPT:http-user-agent=Bearer-\(resolvedDiagnosticSecret)\n"
      out +=
        "#EXTVLCOPT:http-referrer=https://portal.example/\(resolvedDiagnosticSecret)\n"
      out += "http://host.example/live/\(resolvedDiagnosticSecret)/\(i).ts\n"
    } else {
      out += "http://host.example/live/\(i).ts\n"
    }
  }
    return out
}

func sourceId(_ source: Source) -> Int64 {
    guard case let .m3uUrl(id, _, _, _) = source else { fail("expected an m3u-url source") }
    return id
}

// MARK: - The flow

let dbPath = NSTemporaryDirectory() + "spidola-harness-\(UUID().uuidString).sqlite"
let sink = RecordingSink()
let core: Core
do {
    core = try Core(
        config: CoreConfig(dbPath: dbPath, logDirectives: "debug"),
        secrets: MemorySecrets(),
        logSink: sink
    )
} catch {
    fail("core init: \(error)")
}

// 1) Startup handshake crosses the boundary with sane versions.
let handshake = core.handshake()
check(
    handshake.boundaryVersion >= 1 && handshake.schemaVersion >= 1 && !handshake.coreVersion.isEmpty,
    "handshake: \(handshake)"
)

// 2) Error mapping is representable on the Swift side.
do {
    _ = try await core.sources().addM3uUrl(
        name: "Bad", url: "not a url", userAgent: nil, acceptInvalidTls: false)
    fail("expected InvalidInput for a malformed URL")
} catch let error as ApiError {
    guard case .InvalidInput = error else { fail("expected InvalidInput, got \(error)") }
} catch {
    fail("unexpected error type: \(error)")
}

// 3) Import a fixture playlist through the boundary with progress callbacks.
let importBody = Array(playlist(2000).utf8)
let importUrl = startStub(body: importBody, chunk: 8192, delayUs: 0)
let source = try await core.sources().addM3uUrl(
    name: "Fixture", url: importUrl, userAgent: nil, acceptInvalidTls: false)
let id = sourceId(source)
let collector = Collector()
_ = core.sources().refresh(id: id, listener: collector)
switch collector.awaitTerminal() {
case let .complete(outcome):
    check(outcome.inserted == 2000, "expected 2000 inserted, got \(outcome.inserted)")
case let .failed(error):
    fail("import failed: \(error)")
}
check(collector.progressCount >= 1, "no progress callbacks were delivered")

let count = try await core.catalog().channelCount(sourceId: id)
check(count == 2000, "catalog count \(count) != 2000")
check(sink.targets.contains("spidola::import"), "log sink never saw import records")

// The raw generated boundary objects must not reveal plaintext through default native
// diagnostics before CoreKit has a chance to adapt them.
let first = try await core.catalog().channels(sourceId: id, offset: 0, limit: 1).channels[0]
let resolved = try await core.sources().resolvePlayback(
    sourceId: id, identity: first.identity, locator: first.locator)
let resolvedHeaders = resolved.headers()
check(
    resolved.locator().contains(resolvedDiagnosticSecret),
    "resolved locator did not cross the generated boundary")
check(
    resolved.userAgent()?.contains(resolvedDiagnosticSecret) == true,
    "resolved user-agent did not cross the generated boundary")
check(
    resolvedHeaders[0].value().contains(resolvedDiagnosticSecret),
    "resolved header did not cross the generated boundary")
check(
    !String(reflecting: resolved).contains(resolvedDiagnosticSecret)
        && !String(reflecting: resolvedHeaders[0]).contains(resolvedDiagnosticSecret),
    "generated Swift diagnostics exposed a resolved credential")

// 4) Cancel a slow import mid-stream; nothing partial is committed.
let slowBody = Array(playlist(6000).utf8)
let slowUrl = startStub(body: slowBody, chunk: 4096, delayUs: 3000)
let slowSource = try await core.sources().addM3uUrl(
    name: "Slow", url: slowUrl, userAgent: nil, acceptInvalidTls: false)
let slowId = sourceId(slowSource)
let canceller = Collector()
let handle = core.sources().refresh(id: slowId, listener: canceller)
canceller.awaitFirstProgress()
handle.cancel()
switch canceller.awaitTerminal() {
case .failed(.Cancelled):
    break
case let other:
    fail("expected a Cancelled terminal, got \(other)")
}
let slowCount = try await core.catalog().channelCount(sourceId: slowId)
check(slowCount == 0, "cancel left a partial catalog: \(slowCount)")

print(
    "HARNESS OK — handshake=\(handshake.coreVersion)/schema\(handshake.schemaVersion)/boundary\(handshake.boundaryVersion), "
        + "import=2000 progress>=\(collector.progressCount), cancel=Cancelled, logSink+secrets wired"
)
exit(0)
