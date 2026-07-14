// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.player.enginempv

import org.junit.jupiter.api.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

/**
 * The §12 invariant: no credential mpv logs can reach logcat.
 *
 * These assert on *absence of secrets*, not just on the redacted shape, because the failure
 * this guards against is a leak, and a leak is something being present that should not be.
 */
class MpvLogRedactionTest {
    @Test
    fun `strips url userinfo`() {
        val redacted = MpvLogRedaction.redact("Opening http://alice:hunter2@example.com/stream.ts")
        assertFalse(redacted.contains("alice"), redacted)
        assertFalse(redacted.contains("hunter2"), redacted)
        assertTrue(redacted.contains("example.com"), redacted)
    }

    @Test
    fun `strips xtream credentials embedded in the path`() {
        // The case a blocklist-style redactor misses: nothing about /live/u/p/1.ts looks like
        // a secret, but the two path segments are the account.
        val redacted = MpvLogRedaction.redact("[ffmpeg] http://box.example:8080/live/myuser/mypass/1234.ts: 404")
        assertFalse(redacted.contains("myuser"), redacted)
        assertFalse(redacted.contains("mypass"), redacted)
        assertTrue(redacted.contains("box.example:8080"), redacted)
    }

    @Test
    fun `strips query credentials`() {
        val redacted = MpvLogRedaction.redact("GET http://host/live?username=bob&password=s3cret")
        assertFalse(redacted.contains("bob"), redacted)
        assertFalse(redacted.contains("s3cret"), redacted)
    }

    @Test
    fun `keeps the host so the failure is still diagnosable`() {
        // Redaction that destroyed all diagnostic value would just get switched off.
        val redacted = MpvLogRedaction.redact("tcp: Connection refused for http://cdn.example.com:8080/a/b")
        assertTrue(redacted.contains("cdn.example.com:8080"), redacted)
        assertTrue(redacted.contains("Connection refused"), redacted)
    }

    @Test
    fun `an at sign later in the path is not mistaken for userinfo`() {
        val redacted = MpvLogRedaction.redact("http://example.com/path@weird/x.ts")
        assertTrue(redacted.contains("example.com"), redacted)
        assertFalse(redacted.contains("weird"), redacted)
    }

    @Test
    fun `a bare host url with no path is left as scheme and host`() {
        assertEquals("Opening http://example.com", MpvLogRedaction.redact("Opening http://example.com"))
    }

    @Test
    fun `redacts every url on a line`() {
        val redacted = MpvLogRedaction.redact("redirect http://a:b@one.example/x -> http://c:d@two.example/y")
        assertFalse(redacted.contains("a:b"), redacted)
        assertFalse(redacted.contains("c:d"), redacted)
        assertTrue(redacted.contains("one.example"), redacted)
        assertTrue(redacted.contains("two.example"), redacted)
    }

    @Test
    fun `redacts header field assignments by value keeping the name`() {
        val redacted = MpvLogRedaction.redact("Option http-header-fields=Authorization: Bearer abc123")
        assertFalse(redacted.contains("abc123"), redacted)
        assertTrue(redacted.contains("http-header-fields"), redacted)
    }

    @Test
    fun `redacts a whole header value, not just its first word`() {
        // Regression: an earlier tail of \S+ stopped at the first space, so this redacted the
        // word "Authorization:" and left the bearer token itself in the log.
        val redacted = MpvLogRedaction.redact("http-header-fields=Authorization: Bearer eyJhbGciOi.SECRET.sig")
        assertFalse(redacted.contains("SECRET"), redacted)
        assertFalse(redacted.contains("eyJhbGciOi"), redacted)
        assertFalse(redacted.contains("Bearer"), redacted)
    }

    @Test
    fun `redacts a cookie value containing spaces and commas`() {
        val redacted = MpvLogRedaction.redact("Cookie: session=abc def, other=xyz")
        assertFalse(redacted.contains("abc"), redacted)
        assertFalse(redacted.contains("xyz"), redacted)
    }

    @Test
    fun `a secret assignment on one line does not eat the next line`() {
        val redacted = MpvLogRedaction.redact("user-agent=Secret/1.0\n[ffmpeg] h264: hardware decoding")
        assertFalse(redacted.contains("Secret/1.0"), redacted)
        assertTrue(redacted.contains("hardware decoding"), redacted)
    }

    @Test
    fun `redacts a user agent assignment`() {
        val redacted = MpvLogRedaction.redact("user-agent=SecretClient/1.0-token-xyz")
        assertFalse(redacted.contains("token-xyz"), redacted)
    }

    @Test
    fun `leaves an ordinary line untouched`() {
        val line = "[ffmpeg] h264: using hardware decoding"
        assertEquals(line, MpvLogRedaction.redact(line))
    }

    @Test
    fun `is total over odd input`() {
        // mpv's log is not a place to discover a crash in the redactor.
        listOf("", "   ", "://", "http://", "https://@", "not a url at all", "a://b@")
            .forEach { MpvLogRedaction.redact(it) }
    }

    @Test
    fun `handles rtmp and other schemes not just http`() {
        val redacted = MpvLogRedaction.redact("rtmp://user:pw@live.example/app/key")
        assertFalse(redacted.contains("user:pw"), redacted)
        assertFalse(redacted.contains("key"), redacted)
        assertTrue(redacted.contains("live.example"), redacted)
    }
}
