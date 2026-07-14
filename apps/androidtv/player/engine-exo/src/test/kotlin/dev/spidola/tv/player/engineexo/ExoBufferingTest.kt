// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.player.engineexo

import dev.spidola.tv.core.playercontract.BufferingProfile
import org.junit.jupiter.api.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

class ExoBufferingTest {
    /**
     * `DefaultLoadControl.Builder.build()` throws when these orderings are violated, and it throws
     * inside `load`, on the zap path, on a device. Asserting the data here turns a crash into a
     * failing JVM test.
     */
    @Test
    fun `every profile satisfies the load control ordering`() {
        BufferingProfile.entries.forEach { profile ->
            val durations = profile.durations
            assertTrue(
                durations.bufferForPlaybackMs <= durations.minBufferMs,
                "$profile: bufferForPlayback must not exceed minBuffer",
            )
            assertTrue(
                durations.bufferForPlaybackAfterRebufferMs <= durations.minBufferMs,
                "$profile: bufferForPlaybackAfterRebuffer must not exceed minBuffer",
            )
            assertTrue(
                durations.minBufferMs <= durations.maxBufferMs,
                "$profile: minBuffer must not exceed maxBuffer",
            )
        }
    }

    @Test
    fun `every profile is positive`() {
        BufferingProfile.entries.forEach { profile ->
            val durations = profile.durations
            assertTrue(durations.bufferForPlaybackMs > 0, "$profile: bufferForPlayback must be positive")
            assertTrue(durations.minBufferMs > 0, "$profile: minBuffer must be positive")
        }
    }

    /**
     * The profiles must actually order themselves the way their labels promise, or the settings
     * screen is lying: LOW starts soonest and cushions least, GENEROUS the inverse.
     */
    @Test
    fun `the profiles trade start-up against cushion monotonically`() {
        val low = BufferingProfile.LOW.durations
        val balanced = BufferingProfile.BALANCED.durations
        val generous = BufferingProfile.GENEROUS.durations

        assertTrue(low.bufferForPlaybackMs < balanced.bufferForPlaybackMs)
        assertTrue(balanced.bufferForPlaybackMs < generous.bufferForPlaybackMs)

        assertTrue(low.maxBufferMs < balanced.maxBufferMs)
        assertTrue(balanced.maxBufferMs < generous.maxBufferMs)
    }

    /**
     * PRD §9 gives the zap two seconds to first frame. Pre-roll is only one term in that budget —
     * DNS, connect, and manifest fetch share it — so the profiles that claim a fast start must
     * leave room for the rest.
     */
    @Test
    fun `the fast-start profiles keep pre-roll inside the zap budget`() {
        assertTrue(
            BufferingProfile.LOW.durations.bufferForPlaybackMs <= 500,
            "LOW must leave the zap budget almost entirely to the network",
        )
        assertTrue(
            BufferingProfile.BALANCED.durations.bufferForPlaybackMs <= 1_000,
            "BALANCED must still fit the two-second budget on a healthy source",
        )
    }

    @Test
    fun `each profile maps to its own durations`() {
        assertEquals(BufferingProfile.LOW.durations, BufferingProfile.LOW.durations)
        val distinct = BufferingProfile.entries.map { it.durations }.distinct()
        assertEquals(BufferingProfile.entries.size, distinct.size, "profiles must not collapse onto one another")
    }
}
