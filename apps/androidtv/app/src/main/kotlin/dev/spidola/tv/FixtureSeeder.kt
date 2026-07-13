// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv

import android.util.Log
import dev.spidola.tv.core.corekit.ImportEvent
import dev.spidola.tv.core.corekit.SpidolaCore
import dev.spidola.tv.core.corekit.id
import java.io.Closeable
import java.net.InetAddress
import java.net.ServerSocket
import kotlin.concurrent.thread

/**
 * Seeds the M0 walking skeleton with a fixture catalog so the browse screen renders channels on
 * first run. It exercises the full boundary — parse → DB → catalog — by serving a bundled M3U on
 * loopback and importing it through the core exactly as a real source would be. This is skeleton
 * scaffolding: the real add-source flow (M3U URL / file / Xtream) replaces it in Phase 4.
 */
class FixtureSeeder(
    private val core: SpidolaCore,
) {
    suspend fun seedIfNeeded() {
        if (core.sources().isNotEmpty()) return
        LoopbackPlaylistServer(fixturePlaylist().toByteArray()).use { server ->
            val source = core.addM3uUrl("Fixture Catalog", server.start())
            core.import(source.id).collect { event ->
                when (event) {
                    is ImportEvent.Progress -> Unit
                    is ImportEvent.Complete -> Log.i(TAG, "seeded ${event.outcome.inserted} channels")
                    is ImportEvent.Failed -> {
                        Log.w(TAG, "fixture import failed", event.error)
                        throw event.error
                    }
                }
            }
        }
    }

    private fun fixturePlaylist(): String =
        buildString {
            append("#EXTM3U\n")
            for (index in 1..FIXTURE_CHANNEL_COUNT) {
                append("#EXTINF:-1 tvg-id=\"ch$index\" group-title=\"Fixture\",Channel $index\n")
                append("http://host.example/live/$index.ts\n")
            }
        }

    private companion object {
        const val TAG = "spidola::boot"
        const val FIXTURE_CHANNEL_COUNT = 24
    }
}

/** A one-shot loopback HTTP server that serves [body] once, used only to seed the M0 fixture. */
private class LoopbackPlaylistServer(
    private val body: ByteArray,
) : Closeable {
    private val server = ServerSocket(0, BACKLOG, InetAddress.getByName(LOOPBACK))

    fun start(): String {
        thread(isDaemon = true, name = "spidola-fixture") {
            runCatching {
                server.accept().use { socket ->
                    socket.getInputStream().read(ByteArray(REQUEST_BUFFER))
                    socket.getOutputStream().apply {
                        write(responseHeader().toByteArray())
                        write(body)
                        flush()
                    }
                }
            }
        }
        return "http://$LOOPBACK:${server.localPort}/fixture.m3u"
    }

    private fun responseHeader(): String =
        "HTTP/1.1 200 OK\r\n" +
            "Content-Type: application/x-mpegurl\r\n" +
            "Content-Length: ${body.size}\r\n" +
            "Connection: close\r\n\r\n"

    override fun close() {
        runCatching { server.close() }
    }

    private companion object {
        const val LOOPBACK = "127.0.0.1"
        const val BACKLOG = 1
        const val REQUEST_BUFFER = 2048
    }
}
