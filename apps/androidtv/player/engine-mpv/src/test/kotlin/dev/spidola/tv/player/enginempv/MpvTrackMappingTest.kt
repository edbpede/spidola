// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.player.enginempv

import dev.spidola.tv.core.playercontract.TrackId
import dev.spidola.tv.core.playercontract.TrackKind
import org.junit.jupiter.api.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull
import kotlin.test.assertTrue

class MpvTrackMappingTest {
    private fun row(
        id: Int,
        type: String,
        title: String? = null,
        lang: String? = null,
        selected: Boolean = false,
    ) = MpvTrackMapping.RawTrack(id, type, title, lang, selected)

    @Test
    fun `drops video tracks because the contract has no video kind`() {
        val selection =
            MpvTrackMapping.toTrackSelection(
                listOf(row(1, "video"), row(1, "audio"), row(1, "sub")),
            )
        assertEquals(2, selection.available.size)
        assertTrue(selection.available.none { it.kind == TrackKind.AUDIO && it.label == "video" })
    }

    @Test
    fun `audio and subtitle tracks sharing an mpv id stay distinct`() {
        // mpv numbers audio and subtitle tracks in separate sequences, so id 1 exists twice.
        // A bare TrackId("1") would make the contract's own lookup ambiguous and selecting a
        // subtitle would switch the audio instead.
        val selection = MpvTrackMapping.toTrackSelection(listOf(row(1, "audio"), row(1, "sub")))
        val ids = selection.available.map { it.id }
        assertEquals(ids.toSet().size, ids.size, "ids collided: $ids")
    }

    @Test
    fun `selection round-trips to the right mpv property`() {
        assertEquals("aid" to "3", MpvTrackMapping.selectionFor(MpvTrackMapping.trackId(TrackKind.AUDIO, 3)))
        assertEquals("sid" to "2", MpvTrackMapping.selectionFor(MpvTrackMapping.trackId(TrackKind.SUBTITLE, 2)))
    }

    @Test
    fun `a foreign track id is rejected rather than guessed`() {
        // FakeEngine ids, or a persisted id from another engine, must not set a random property.
        assertNull(MpvTrackMapping.selectionFor(TrackId("1")))
        assertNull(MpvTrackMapping.selectionFor(TrackId("video:1")))
        assertNull(MpvTrackMapping.selectionFor(TrackId("aid:notanumber")))
        assertNull(MpvTrackMapping.selectionFor(TrackId("")))
    }

    @Test
    fun `reports the selected audio and subtitle`() {
        val selection =
            MpvTrackMapping.toTrackSelection(
                listOf(
                    row(1, "audio"),
                    row(2, "audio", selected = true),
                    row(1, "sub", selected = true),
                ),
            )
        assertEquals(MpvTrackMapping.trackId(TrackKind.AUDIO, 2), selection.selectedAudio)
        assertEquals(MpvTrackMapping.trackId(TrackKind.SUBTITLE, 1), selection.selectedSubtitle)
    }

    @Test
    fun `nothing selected is reported as nothing selected`() {
        val selection = MpvTrackMapping.toTrackSelection(listOf(row(1, "audio"), row(1, "sub")))
        assertNull(selection.selectedAudio)
        assertNull(selection.selectedSubtitle)
    }

    @Test
    fun `prefers the title as the label`() {
        val selection = MpvTrackMapping.toTrackSelection(listOf(row(1, "audio", title = "Director commentary")))
        assertEquals("Director commentary", selection.available.single().label)
    }

    @Test
    fun `falls back to language when there is no title`() {
        val selection = MpvTrackMapping.toTrackSelection(listOf(row(1, "audio", lang = "eng")))
        assertEquals("eng", selection.available.single().label)
    }

    @Test
    fun `falls back to an ordinal when mpv offers nothing`() {
        // Most real IPTV tracks arrive with neither title nor language.
        val selection = MpvTrackMapping.toTrackSelection(listOf(row(1, "audio"), row(2, "audio")))
        assertEquals(listOf("Audio 1", "Audio 2"), selection.available.map { it.label })
    }

    @Test
    fun `ordinals count within a kind, not across kinds`() {
        val selection = MpvTrackMapping.toTrackSelection(listOf(row(1, "audio"), row(1, "sub"), row(2, "sub")))
        assertEquals(listOf("Audio 1", "Subtitle 1", "Subtitle 2"), selection.available.map { it.label })
    }

    @Test
    fun `blank title and language fall through to the ordinal`() {
        val selection = MpvTrackMapping.toTrackSelection(listOf(row(1, "audio", title = "  ", lang = "")))
        assertEquals("Audio 1", selection.available.single().label)
    }

    @Test
    fun `blank language is reported as no language`() {
        val selection = MpvTrackMapping.toTrackSelection(listOf(row(1, "audio", lang = "  ")))
        assertNull(selection.available.single().language)
    }

    @Test
    fun `an empty track list is empty, not an error`() {
        val selection = MpvTrackMapping.toTrackSelection(emptyList())
        assertTrue(selection.available.isEmpty())
        assertNull(selection.selectedAudio)
    }
}
