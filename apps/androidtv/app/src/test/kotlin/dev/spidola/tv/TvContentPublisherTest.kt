// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv

import dev.spidola.tv.core.corekit.PlayableChannel
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertFalse
import org.junit.jupiter.api.Assertions.assertNull
import org.junit.jupiter.api.Test

class TvContentPublisherTest {
    @Test
    fun `content identity is stable across renamed channels`() {
        val channel = PlayableChannel(7, 99, "BBC One", null, null, "https://example.test/live")

        assertEquals("7:99", channel.contentKey())
        assertEquals(channel.contentKey(), channel.copy(name = "BBC 1").contentKey())
    }

    @Test
    fun `exported TV deep links do not disclose playback locators`() {
        val channel = PlayableChannel(7, 99, "BBC One", null, null, "https://user:secret@example.test/live")

        val deepLink = channelDeepLink(channel)

        assertEquals("spidola://channel?sourceId=7&identity=99", deepLink)
        assertFalse(deepLink.contains("secret"))
    }

    @Test
    fun `stored publisher rows reject malformed provider data`() {
        assertEquals("7:99" to 42L, decodeStoredEntry("7:99|42"))
        assertNull(decodeStoredEntry("7:99"))
        assertNull(decodeStoredEntry("7:99|not-a-row"))
    }
}
