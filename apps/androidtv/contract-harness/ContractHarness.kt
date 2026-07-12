// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

// Kotlin half of the FFI parity keel (TECH_SPEC §5, §10, Phase 2 exit criteria).
//
// A bare test harness — no UI — that drives the SAME fixture flow as the Rust contract test
// (`crates/core-api/tests/contract.rs`) and the Swift harness, against the SAME compiled
// `core-api` library, through the generated Kotlin bindings. It builds a `Core` with a host
// secret store and log sink, imports a fixture playlist over a local HTTP stub while receiving
// progress callbacks, cancels a second import mid-stream, and asserts FFI error variants are
// representable on the Kotlin side. Any assertion failure exits non-zero; a core panic aborts
// the process (a red build). Built and run by the Android CI lane
// (`tools/ci/build-contract-harness-android.sh`) against the host-arch library, with
// `-Djna.library.path` pointed at it.

package dev.spidola.tv.contract

import java.net.InetAddress
import java.net.ServerSocket
import java.util.UUID
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.locks.ReentrantLock
import kotlin.concurrent.thread
import kotlin.concurrent.withLock
import kotlin.system.exitProcess
import kotlinx.coroutines.runBlocking
import uniffi.core_api.ApiException
import uniffi.core_api.Core
import uniffi.core_api.CoreConfig
import uniffi.core_api.ImportListener
import uniffi.core_api.ImportOutcome
import uniffi.core_api.ImportProgress
import uniffi.core_api.LogRecord
import uniffi.core_api.LogSink
import uniffi.core_api.SecretStore
import uniffi.core_api.Source

private fun fail(message: String): Nothing {
    System.err.println("HARNESS FAIL: $message")
    exitProcess(1)
}

private fun ensure(condition: Boolean, message: () -> String) {
    if (!condition) fail(message())
}

// -- Host callback fakes -----------------------------------------------------------------

private class MemorySecrets : SecretStore {
    private val lock = ReentrantLock()
    private val store = HashMap<String, String>()

    override fun get(key: String): String? = lock.withLock { store[key] }

    override fun set(key: String, value: String) {
        lock.withLock { store[key] = value }
    }

    override fun delete(key: String) {
        lock.withLock { store.remove(key) }
    }
}

private class RecordingSink : LogSink {
    private val lock = ReentrantLock()
    private val records = ArrayList<LogRecord>()

    override fun log(record: LogRecord) {
        lock.withLock { records.add(record) }
    }

    fun targets(): List<String> = lock.withLock { records.map { it.target } }
}

private sealed interface Terminal {
    data class Complete(val outcome: ImportOutcome) : Terminal

    data class Failed(val error: ApiException) : Terminal
}

private class Collector : ImportListener {
    private val lock = ReentrantLock()
    private var count = 0
    private var firedFirst = false
    private var terminal: Terminal? = null
    private val firstProgress = CountDownLatch(1)
    private val done = CountDownLatch(1)

    override fun onProgress(progress: ImportProgress) {
        val first =
            lock.withLock {
                count += 1
                val f = !firedFirst
                firedFirst = true
                f
            }
        if (first) firstProgress.countDown()
    }

    override fun onComplete(outcome: ImportOutcome) {
        lock.withLock { terminal = Terminal.Complete(outcome) }
        done.countDown()
    }

    override fun onFailed(error: ApiException) {
        lock.withLock { terminal = Terminal.Failed(error) }
        done.countDown()
    }

    val progressCount: Int
        get() = lock.withLock { count }

    fun awaitFirstProgress() {
        firstProgress.await(60, TimeUnit.SECONDS)
    }

    fun awaitTerminal(): Terminal {
        done.await(60, TimeUnit.SECONDS)
        return lock.withLock { terminal } ?: fail("no terminal outcome delivered")
    }
}

// -- Local HTTP stub ---------------------------------------------------------------------

private fun startStub(body: ByteArray, chunk: Int, delayMs: Long): String {
    val server = ServerSocket(0, 8, InetAddress.getByName("127.0.0.1"))
    val port = server.localPort
    thread(isDaemon = true) {
        server.use { srv ->
            srv.accept().use { socket ->
                val request = ByteArray(2048)
                socket.getInputStream().read(request) // consume request line + headers
                val out = socket.getOutputStream()
                val header =
                    "HTTP/1.1 200 OK\r\nContent-Type: application/x-mpegurl\r\n" +
                        "Content-Length: ${body.size}\r\nConnection: close\r\n\r\n"
                out.write(header.toByteArray())
                var offset = 0
                while (offset < body.size) {
                    val end = minOf(offset + chunk, body.size)
                    out.write(body, offset, end - offset)
                    out.flush()
                    offset = end
                    if (delayMs > 0) Thread.sleep(delayMs)
                }
            }
        }
    }
    return "http://127.0.0.1:$port/playlist.m3u"
}

private fun playlist(count: Int): String =
    buildString {
        append("#EXTM3U\n")
        for (i in 0 until count) {
            append("#EXTINF:-1 tvg-id=\"id$i\" group-title=\"News\",Channel $i\n")
            append("http://host.example/live/$i.ts\n")
        }
    }

private fun sourceId(source: Source): Long =
    when (source) {
        is Source.M3uUrl -> source.id
        is Source.M3uFile -> fail("expected an m3u-url source, got m3u-file")
        is Source.Xtream -> fail("expected an m3u-url source, got xtream")
    }

// -- The flow ----------------------------------------------------------------------------

fun main() {
    val dbPath =
        java.io.File(
            System.getProperty("java.io.tmpdir"),
            "spidola-harness-${UUID.randomUUID()}.sqlite",
        ).absolutePath
    val sink = RecordingSink()
    val core = Core(CoreConfig(dbPath, "debug"), MemorySecrets(), sink)

    // 1) Startup handshake crosses the boundary with sane versions.
    val handshake = core.handshake()
    ensure(
        handshake.boundaryVersion >= 1u &&
            handshake.schemaVersion >= 1u &&
            handshake.coreVersion.isNotEmpty(),
    ) { "handshake: $handshake" }

    runBlocking {
        // 2) Error mapping is representable on the Kotlin side.
        try {
            core.sources().addM3uUrl("Bad", "not a url", null, false)
            fail("expected InvalidInput for a malformed URL")
        } catch (expected: ApiException.InvalidInput) {
            // representable: a typed, catchable sealed error
        }

        // 3) Import a fixture playlist through the boundary with progress callbacks.
        val importBody = playlist(2000).toByteArray()
        val importUrl = startStub(importBody, 8192, 0)
        val source = core.sources().addM3uUrl("Fixture", importUrl, null, false)
        val id = sourceId(source)
        val collector = Collector()
        core.sources().refresh(id, collector)
        when (val terminal = collector.awaitTerminal()) {
            is Terminal.Complete ->
                ensure(terminal.outcome.inserted == 2000uL) {
                    "expected 2000 inserted, got ${terminal.outcome.inserted}"
                }
            is Terminal.Failed -> fail("import failed: ${terminal.error}")
        }
        ensure(collector.progressCount >= 1) { "no progress callbacks were delivered" }

        val count = core.catalog().channelCount(id)
        ensure(count == 2000uL) { "catalog count $count != 2000" }
        ensure(sink.targets().contains("spidola::import")) {
            "log sink never saw import records"
        }

        // 4) Cancel a slow import mid-stream; nothing partial is committed.
        val slowBody = playlist(6000).toByteArray()
        val slowUrl = startStub(slowBody, 4096, 3)
        val slowSource = core.sources().addM3uUrl("Slow", slowUrl, null, false)
        val slowId = sourceId(slowSource)
        val canceller = Collector()
        val handle = core.sources().refresh(slowId, canceller)
        canceller.awaitFirstProgress()
        handle.cancel()
        when (val terminal = canceller.awaitTerminal()) {
            is Terminal.Failed ->
                ensure(terminal.error is ApiException.Cancelled) {
                    "expected Cancelled, got ${terminal.error}"
                }
            is Terminal.Complete -> fail("expected a Cancelled terminal, got Complete")
        }
        val slowCount = core.catalog().channelCount(slowId)
        ensure(slowCount == 0uL) { "cancel left a partial catalog: $slowCount" }
    }

    println(
        "HARNESS OK — handshake=${handshake.coreVersion}/schema${handshake.schemaVersion}/" +
            "boundary${handshake.boundaryVersion}, import=2000, cancel=Cancelled, logSink+secrets wired",
    )
}
