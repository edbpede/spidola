// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.core.playercontract

import kotlinx.collections.immutable.persistentListOf
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNull
import kotlin.test.assertTrue

/**
 * The selection policy (TECH_SPEC §8) and the loud-fallback rule. Pure logic — the Swift mirror
 * (`EngineSelectionTests`) asserts the same cases, which is what keeps "identical on both
 * platforms" from being a slogan.
 */
class EngineSelectionTest {
    private val mpv = EngineId.MPV
    private val exo = EngineId.EXOPLAYER
    private val both = setOf(mpv, exo)

    @Test
    fun `engine request diagnostics redact credentials`() {
        val secret = "credential-value"
        val request =
            StreamRequest(
                locator = "https://stream.example/$secret",
                headers = persistentListOf(StreamHeader("Authorization", secret)),
                userAgent = "Bearer-$secret",
            )

        assertFalse(secret in request.toString())
    }

    // region Precedence: channel → source → platform default

    @Test
    fun `channel override wins over source and default`() {
        val resolved =
            EngineSelection.resolve(
                channelOverride = mpv,
                sourceOverride = exo,
                platformDefault = exo,
                registered = both,
            )
        assertEquals(mpv, resolved)
    }

    @Test
    fun `source override wins when no channel override`() {
        val resolved =
            EngineSelection.resolve(
                channelOverride = null,
                sourceOverride = mpv,
                platformDefault = exo,
                registered = both,
            )
        assertEquals(mpv, resolved)
    }

    @Test
    fun `platform default when no overrides`() {
        val resolved =
            EngineSelection.resolve(
                channelOverride = null,
                sourceOverride = null,
                platformDefault = exo,
                registered = both,
            )
        assertEquals(exo, resolved)
    }

    // endregion

    // region Stale / foreign override keys

    /**
     * Overrides are opaque strings that outlive builds: a key naming an engine this build does not
     * link must never make a channel unplayable.
     */
    @Test
    fun `unregistered channel override falls through to source`() {
        val resolved =
            EngineSelection.resolve(
                channelOverride = EngineId("avplayer"),
                sourceOverride = mpv,
                platformDefault = exo,
                registered = both,
            )
        assertEquals(mpv, resolved)
    }

    @Test
    fun `unregistered overrides fall through to default`() {
        val resolved =
            EngineSelection.resolve(
                channelOverride = EngineId("future-engine"),
                sourceOverride = EngineId("avplayer"),
                platformDefault = exo,
                registered = both,
            )
        assertEquals(exo, resolved)
    }

    /**
     * The default is returned even when unregistered, so the caller reports one honest failure
     * rather than the policy inventing a substitute.
     */
    @Test
    fun `default returned even when not registered`() {
        val resolved =
            EngineSelection.resolve(
                channelOverride = null,
                sourceOverride = null,
                platformDefault = exo,
                registered = emptySet(),
            )
        assertEquals(exo, resolved)
    }

    // endregion

    // region "Try other player" target

    @Test
    fun `alternate is the other registered engine`() {
        assertEquals(mpv, EngineSelection.alternate(exo, both))
        assertEquals(exo, EngineSelection.alternate(mpv, both))
    }

    @Test
    fun `alternate is null when nothing else is registered`() {
        assertNull(EngineSelection.alternate(exo, setOf(exo)))
    }

    /**
     * A non-deterministic offer would make "remember for this channel" remember a choice the viewer
     * did not make.
     */
    @Test
    fun `alternate is deterministic`() {
        val registered = setOf(mpv, exo, EngineId("zzz"))
        val first = EngineSelection.alternate(exo, registered)
        repeat(32) { assertEquals(first, EngineSelection.alternate(exo, registered)) }
    }

    // endregion

    // region Loud fallback

    /** Only a format/decode failure means another engine could plausibly succeed (TECH_SPEC §8). */
    @Test
    fun `only format and decode failures offer another player`() {
        assertTrue(EngineError.UnsupportedFormat.offersOtherPlayer)
        assertTrue(EngineError.DecoderFailed.offersOtherPlayer)
        assertFalse(EngineError.SourceUnreachable.offersOtherPlayer)
        assertFalse(EngineError.Unauthorized.offersOtherPlayer)
        assertFalse(EngineError.Timeout.offersOtherPlayer)
        assertFalse(EngineError.Unknown("boom").offersOtherPlayer)
    }

    /**
     * Every variant, exhaustively: adding one forces a UX decision here rather than shipping a
     * blank screen (PRD §6.3 — an error with no action is a design bug).
     */
    @Test
    fun `every error variant has couch-legible copy`() {
        val all =
            listOf(
                EngineError.SourceUnreachable,
                EngineError.Unauthorized,
                EngineError.UnsupportedFormat,
                EngineError.DecoderFailed,
                EngineError.Timeout,
                EngineError.Unknown("detail"),
            )
        for (error in all) {
            assertTrue(error.failureClass.isNotEmpty(), "$error has no failure class")
            assertTrue(error.message.isNotEmpty(), "$error has no message")
        }
    }

    /** Diagnostic chains go to the log stream, never the screen (PRD §8.6). */
    @Test
    fun `only unknown carries diagnostic detail`() {
        assertEquals("mpv: -10", EngineError.Unknown("mpv: -10").diagnosticDetail)
        assertNull(EngineError.DecoderFailed.diagnosticDetail)
    }

    // endregion
}
