// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.player.engineexo

import dev.spidola.tv.core.playercontract.TrackId
import org.junit.jupiter.api.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull

/**
 * The track id is the seam between the contract's opaque handle and ExoPlayer's two-index track
 * model. If the round trip is lossy, [ExoEngine.select] silently picks the wrong track or none.
 */
class ExoTrackIdTest {
    @Test
    fun `coordinates survive the round trip`() {
        val cases =
            listOf(
                TrackCoordinates(groupIndex = 0, trackIndex = 0),
                TrackCoordinates(groupIndex = 3, trackIndex = 0),
                TrackCoordinates(groupIndex = 0, trackIndex = 7),
                TrackCoordinates(groupIndex = 12, trackIndex = 4),
                TrackCoordinates(groupIndex = Int.MAX_VALUE, trackIndex = Int.MAX_VALUE),
            )

        cases.forEach { coordinates ->
            assertEquals(coordinates, coordinates.encode().decode(), "round trip failed for $coordinates")
        }
    }

    @Test
    fun `the encoding is the documented group and track pair`() {
        assertEquals(TrackId("3:1"), TrackCoordinates(groupIndex = 3, trackIndex = 1).encode())
    }

    /**
     * Ids reach `select` from UI state that a track change may already have invalidated, so
     * decoding validates rather than trusts.
     */
    @Test
    fun `malformed ids decode to null`() {
        val malformed =
            listOf(
                "",
                "3",
                "3:",
                ":1",
                "3:1:2",
                "a:1",
                "3:b",
                "-1:0",
                "0:-1",
                "3 : 1",
                "3.0:1",
            )

        malformed.forEach { value ->
            assertNull(TrackId(value).decode(), "'$value' should not decode")
        }
    }
}
