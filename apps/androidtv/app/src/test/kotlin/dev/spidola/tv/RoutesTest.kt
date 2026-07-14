// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv

import dev.spidola.tv.core.corekit.PlayableChannel
import dev.spidola.tv.core.corekit.ZapContext
import kotlinx.serialization.json.Json
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Test
import uniffi.core_api.MediaKind

/**
 * The back stack is restored from bytes after process death, so a route that cannot survive the
 * round trip loses the viewer's ring — and zap lands them on a channel they never chose (PRD §8.4).
 * These cover the hand-written half of that: the [ZapContext] mirror and the encoding of every arm.
 */
class RoutesTest {
    @Test
    fun `every zap context arm survives the route mirror`() {
        val contexts =
            listOf(
                ZapContext.Group(sourceId = 7, kind = MediaKind.LIVE, group = "News"),
                // A group ring with no group is the source's ungrouped channels, not a missing value.
                ZapContext.Group(sourceId = 7, kind = MediaKind.SERIES_EPISODE, group = null),
                ZapContext.Favorites,
                ZapContext.Search(query = "bbc", sourceId = 3, kind = MediaKind.MOVIE),
                // The unfiltered search: both filters absent must stay absent, not collapse to a default.
                ZapContext.Search(query = "bbc", sourceId = null, kind = null),
                ZapContext.Single,
            )

        contexts.forEach { context ->
            assertEquals(context, ZapContextRoute.of(context).toContext())
        }
    }

    @Test
    fun `a playback route survives encoding with its ring and offset intact`() {
        val route =
            PlaybackRoute(
                channel = ChannelPayload.of(channel()),
                context = ZapContextRoute.of(ZapContext.Group(sourceId = 7, kind = MediaKind.LIVE, group = "News")),
                offset = 41u,
            )

        val restored = Json.decodeFromString<PlaybackRoute>(Json.encodeToString(route))

        assertEquals(route, restored)
    }

    @Test
    fun `playing what the detail screen shows keeps the ring it was opened with`() {
        val detail =
            ChannelRoute.of(
                channel = channel(),
                context = ZapContext.Search(query = "bbc", sourceId = null, kind = null),
                offset = 12u,
            )

        val playback = PlaybackRoute.of(detail)

        assertEquals(detail.channel, playback.channel)
        assertEquals(detail.context, playback.context)
        assertEquals(detail.offset, playback.offset)
    }

    @Test
    fun `a channel survives the route payload`() {
        val channel = channel()

        assertEquals(channel, ChannelPayload.of(channel).toPlayable())
    }

    @Test
    fun `a channel with no kind keeps it absent through the route payload`() {
        // A channel opened from a recent carries no kind, and the payload must not invent one.
        val channel = channel().copy(kind = null)

        assertEquals(channel, ChannelPayload.of(channel).toPlayable())
    }

    private fun channel(): PlayableChannel =
        PlayableChannel(
            sourceId = 7,
            identity = 99,
            name = "BBC One",
            group = "News",
            logo = "http://host.example/logo.png",
            locator = "http://host.example/live/1.ts",
            kind = MediaKind.LIVE,
        )
}
