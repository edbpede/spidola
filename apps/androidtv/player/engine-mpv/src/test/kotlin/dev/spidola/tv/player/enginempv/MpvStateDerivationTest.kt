// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.player.enginempv

import dev.spidola.tv.core.playercontract.PlaybackState
import dev.spidola.tv.core.playercontract.isTerminal
import org.junit.jupiter.api.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse

class MpvStateDerivationTest {
    private fun flags(
        fileLoaded: Boolean = true,
        pause: Boolean = false,
        pausedForCache: Boolean = false,
        coreIdle: Boolean = false,
    ) = MpvStateDerivation.Flags(fileLoaded, pause, pausedForCache, coreIdle)

    @Test
    fun `before file loaded it is loading, whatever else is set`() {
        // core-idle is trivially true while opening; checking it first would report Buffering
        // for the whole click-to-first-frame window PRD §9 measures as Loading.
        assertEquals(
            PlaybackState.Loading,
            MpvStateDerivation.stateFor(flags(fileLoaded = false, coreIdle = true)),
        )
    }

    @Test
    fun `playing when nothing is holding it back`() {
        assertEquals(PlaybackState.Playing, MpvStateDerivation.stateFor(flags()))
    }

    @Test
    fun `paused when the viewer paused`() {
        assertEquals(PlaybackState.Paused, MpvStateDerivation.stateFor(flags(pause = true)))
    }

    @Test
    fun `buffering when starved`() {
        assertEquals(PlaybackState.Buffering, MpvStateDerivation.stateFor(flags(pausedForCache = true)))
    }

    @Test
    fun `buffering when core is idle without an explicit pause`() {
        assertEquals(PlaybackState.Buffering, MpvStateDerivation.stateFor(flags(coreIdle = true)))
    }

    @Test
    fun `an explicit pause outranks starvation`() {
        // Both are true whenever a starved stream is paused. The viewer who pressed pause
        // should see Paused, not a spinner implying the app is still trying.
        assertEquals(
            PlaybackState.Paused,
            MpvStateDerivation.stateFor(flags(pause = true, pausedForCache = true, coreIdle = true)),
        )
    }

    @Test
    fun `never derives a terminal state`() {
        // Ended and Failed are owned by END_FILE; no flag combination may produce them, or a
        // momentarily quiet stream would look like a dead one.
        val combinations =
            listOf(true, false).flatMap { a ->
                listOf(true, false).flatMap { b ->
                    listOf(true, false).flatMap { c ->
                        listOf(true, false).map { d -> MpvStateDerivation.Flags(a, b, c, d) }
                    }
                }
            }
        combinations.forEach { f ->
            assertFalse(MpvStateDerivation.stateFor(f).isTerminal, "terminal state derived from $f")
        }
    }

    @Test
    fun `yes is the only truthy mpv flag`() {
        assertEquals(true, MpvStateDerivation.flagOf("yes"))
        assertEquals(false, MpvStateDerivation.flagOf("no"))
    }

    @Test
    fun `an unavailable property is false, never assumed set`() {
        assertEquals(false, MpvStateDerivation.flagOf(null))
        assertEquals(false, MpvStateDerivation.flagOf(""))
        assertEquals(false, MpvStateDerivation.flagOf("true"))
        assertEquals(false, MpvStateDerivation.flagOf("1"))
    }
}
