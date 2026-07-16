// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv

import android.content.Intent
import dev.spidola.tv.core.corekit.PlayableChannel
import dev.spidola.tv.core.corekit.ZapContext
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.Json
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertFalse
import org.junit.jupiter.api.Test

class PlatformRouteTest {
    @Test
    fun `system search opens search with the spoken query`() {
        assertEquals(
            SearchRoute("world news"),
            platformStartRoute(Intent.ACTION_SEARCH, "world news", null, null),
        )
    }

    @Test
    fun `search deep link opens search without trusting unrelated hosts`() {
        assertEquals(
            SearchRoute("arte"),
            platformStartRoute(Intent.ACTION_VIEW, null, "search", "arte"),
        )
        assertEquals(
            HomeRoute,
            platformStartRoute(Intent.ACTION_VIEW, null, "unknown", "arte"),
        )
    }

    @Test
    fun `TV provider channel link opens the matching channel detail`() {
        val channel =
            PlayableChannel(
                sourceId = 7,
                identity = 99,
                name = "BBC One",
                group = "News",
                logo = "https://example.test/bbc.png",
                locator = "https://example.test/live",
                kind = uniffi.core_api.MediaKind.LIVE,
            )
        val route =
            platformChannelRoute(
                sourceId = "7",
                identity = "99",
                channelLookup = { _, _ -> channel },
            )

        assertEquals(
            ChannelRoute.of(
                channel,
                ZapContext.Single,
                0u,
            ),
            route,
        )
    }

    @Test
    fun `malformed TV provider channel link falls back to home`() {
        assertEquals(
            HomeRoute,
            platformChannelRoute("not-an-id", "99") { _, _ -> error("must not resolve malformed ids") },
        )
        assertEquals(
            HomeRoute,
            platformChannelRoute("7", "99") { _, _ -> null },
        )
    }

    @Test
    fun `channel deep link survives provider process recreation`() =
        runBlocking {
            val channel =
                PlayableChannel(
                    sourceId = 7,
                    identity = 99,
                    name = "BBC One",
                    group = "News",
                    logo = null,
                    locator = "sealed",
                    kind = uniffi.core_api.MediaKind.LIVE,
                )

            val route =
                platformChannelRouteAfterRestart(
                    sourceId = "7",
                    identity = "99",
                    cachedLookup = { _, _ -> null },
                    persistentLookup = { source, identity ->
                        channel.takeIf { it.sourceId == source && it.identity == identity }
                    },
                )

            assertEquals(ChannelRoute.of(channel, ZapContext.Single, 0u), route)
        }

    @Test
    fun `custom playback route serializes display metadata but no request secrets`() {
        val secret = "plain-text-stream-secret"
        val encoded = Json.encodeToString(CustomPlaybackRoute(42L, "Community TV", null))

        assertEquals(
            CustomPlaybackRoute(42L, "Community TV", null),
            Json.decodeFromString<CustomPlaybackRoute>(encoded),
        )
        assertFalse(secret in encoded)
        assertFalse("locator" in encoded)
        assertFalse("headers" in encoded)
        assertFalse("userAgent" in encoded)
    }
}
