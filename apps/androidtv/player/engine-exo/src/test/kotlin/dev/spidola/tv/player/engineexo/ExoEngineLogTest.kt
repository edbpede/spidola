// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.player.engineexo

import dev.spidola.tv.core.playercontract.BufferingProfile
import dev.spidola.tv.core.playercontract.StreamHeader
import dev.spidola.tv.core.playercontract.StreamRequest
import kotlinx.collections.immutable.persistentListOf
import org.junit.jupiter.api.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

/**
 * TECH_SPEC §12 makes "secrets never enter log messages" a hard invariant. These tests are that
 * invariant, executable.
 */
class ExoEngineLogTest {
    @Test
    fun `an xtream locator loses its credentials`() {
        val redacted = redactLocator("http://iptv.example.com:8080/live/subscriber/hunter2/931.ts")

        assertEquals("http://iptv.example.com:8080/…", redacted)
    }

    @Test
    fun `userinfo credentials are dropped`() {
        val redacted = redactLocator("https://subscriber:hunter2@iptv.example.com/stream.m3u8")

        assertFalse(redacted.contains("hunter2"), "redacted locator leaked a password: '$redacted'")
        assertFalse(redacted.contains("subscriber"), "redacted locator leaked a username: '$redacted'")
        assertEquals("https://iptv.example.com/…", redacted)
    }

    @Test
    fun `query strings are dropped`() {
        val redacted = redactLocator("https://iptv.example.com/stream.m3u8?token=hunter2")

        assertFalse(redacted.contains("hunter2"), "redacted locator leaked a token: '$redacted'")
    }

    @Test
    fun `a portless locator omits the port`() {
        assertEquals("https://iptv.example.com/…", redactLocator("https://iptv.example.com/live/931.ts"))
    }

    /**
     * The unparsable case is the one most likely to be both malformed and credential-bearing, so it
     * must not degrade to echoing the raw string.
     */
    @Test
    fun `an unparsable locator degrades to a marker`() {
        assertEquals("<unparsable>", redactLocator("http://[not a uri/live/subscriber/hunter2"))
    }

    @Test
    fun `a hostless locator degrades to a marker`() {
        assertEquals("<opaque>", redactLocator("rtp:hunter2"))
    }

    @Test
    fun `the request summary reports header names but never values`() {
        val request =
            StreamRequest(
                locator = "http://iptv.example.com/live/subscriber/hunter2/931.ts",
                headers =
                    persistentListOf(
                        StreamHeader(name = "Authorization", value = "Bearer sk-live-secret"),
                        StreamHeader(name = "X-Session", value = "session-secret"),
                    ),
                userAgent = "SpidolaTV/1.0 token-bearing-agent",
                buffering = BufferingProfile.LOW,
            )

        val summary = request.logSummary()

        assertTrue(summary.contains("Authorization"), "summary should name the header: '$summary'")
        assertTrue(summary.contains("X-Session"), "summary should name the header: '$summary'")
        assertFalse(summary.contains("sk-live-secret"), "summary leaked a header value: '$summary'")
        assertFalse(summary.contains("session-secret"), "summary leaked a header value: '$summary'")
        assertFalse(summary.contains("token-bearing-agent"), "summary leaked the user agent: '$summary'")
        assertFalse(summary.contains("hunter2"), "summary leaked a credential from the locator: '$summary'")
    }

    @Test
    fun `the request summary reports whether a user agent was overridden`() {
        val base = StreamRequest(locator = "http://iptv.example.com/931.ts")

        assertTrue(base.logSummary().contains("userAgent=default"))
        assertTrue(base.copy(userAgent = "SpidolaTV/1.0").logSummary().contains("userAgent=override"))
    }

    @Test
    fun `a headerless request says so`() {
        val summary = StreamRequest(locator = "http://iptv.example.com/931.ts").logSummary()

        assertTrue(summary.contains("headers=[none]"), "summary should be explicit about no headers: '$summary'")
    }
}
