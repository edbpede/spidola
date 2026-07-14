// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.player.enginempv

import dev.spidola.tv.core.playercontract.MediaTrack
import dev.spidola.tv.core.playercontract.TrackId
import dev.spidola.tv.core.playercontract.TrackKind
import dev.spidola.tv.core.playercontract.TrackSelection
import kotlinx.collections.immutable.toImmutableList

/**
 * mpv's `track-list` → the contract's [TrackSelection] (TECH_SPEC §8).
 *
 * Pure, so the track menu the viewer sees is testable without a decoder.
 */
internal object MpvTrackMapping {
    /** mpv's `track-list/N/type` values this engine surfaces. */
    private const val TYPE_AUDIO = "audio"
    private const val TYPE_SUB = "sub"

    /**
     * One row of mpv's `track-list`, read as flat sub-properties
     * (`track-list/N/id`, `.../type`, …) rather than as a single `MPV_FORMAT_NODE`.
     *
     * The flat read is why this type exists: marshalling an mpv node tree across JNI means
     * hand-walking a tagged union, whereas the sub-properties are plain ints, strings and
     * flags that JNI already has accessors for. The cost is N+1 property reads per track
     * list — paid once when a stream opens, never on the zap path's critical section.
     */
    data class RawTrack(
        val id: Int,
        val type: String,
        val title: String?,
        val lang: String?,
        val selected: Boolean,
    )

    /**
     * Builds the contract's track menu from mpv's rows.
     *
     * Video rows are dropped: [TrackKind] has no video case, because no shipped surface
     * selects a video track and an unused arm would be an untested arm.
     */
    fun toTrackSelection(rows: List<RawTrack>): TrackSelection {
        val playable = rows.filter { it.type == TYPE_AUDIO || it.type == TYPE_SUB }

        val tracks =
            playable.map { row ->
                val kind = if (row.type == TYPE_AUDIO) TrackKind.AUDIO else TrackKind.SUBTITLE
                MediaTrack(
                    id = trackId(kind, row.id),
                    kind = kind,
                    label = labelFor(row, kind, playable),
                    language = row.lang?.takeIf { it.isNotBlank() },
                )
            }

        return TrackSelection(
            available = tracks.toImmutableList(),
            selectedAudio =
                playable.firstOrNull { it.type == TYPE_AUDIO && it.selected }
                    ?.let { trackId(TrackKind.AUDIO, it.id) },
            selectedSubtitle =
                playable.firstOrNull { it.type == TYPE_SUB && it.selected }
                    ?.let { trackId(TrackKind.SUBTITLE, it.id) },
        )
    }

    /**
     * The contract handle for an mpv track.
     *
     * The kind is encoded into the id, not merely carried alongside it, because mpv numbers
     * audio and subtitle tracks in **separate** sequences: audio 1 and subtitle 1 both
     * routinely exist. A bare `TrackId("1")` would make the contract's own
     * `available.firstOrNull { it.id == track }` lookup ambiguous, and selecting a subtitle
     * would silently switch the audio instead.
     */
    fun trackId(
        kind: TrackKind,
        mpvId: Int,
    ): TrackId = TrackId("${propertyPrefix(kind)}:$mpvId")

    /**
     * Reads a [TrackId] back into the mpv property and value that select it, or `null` if it
     * did not come from [trackId].
     *
     * Returns the property name (`aid`/`sid`) with the value, so the caller sets one property
     * without re-deriving which one from the kind.
     */
    fun selectionFor(id: TrackId): Pair<String, String>? {
        val prefix = id.value.substringBefore(':', missingDelimiterValue = "")
        val mpvId = id.value.substringAfter(':', missingDelimiterValue = "").toIntOrNull() ?: return null
        return when (prefix) {
            PREFIX_AUDIO -> PROPERTY_AID to mpvId.toString()
            PREFIX_SUB -> PROPERTY_SID to mpvId.toString()
            else -> null
        }
    }

    private fun propertyPrefix(kind: TrackKind): String =
        when (kind) {
            TrackKind.AUDIO -> PREFIX_AUDIO
            TrackKind.SUBTITLE -> PREFIX_SUB
        }

    /**
     * The couch-legible label (PRD §8.6: no system jargon reaches the screen).
     *
     * mpv gives a title for maybe half of real IPTV tracks and a language for most. The
     * ordinal fallback ("Audio 2") is last because it tells the viewer nothing except that
     * a choice exists — which is still better than an empty row.
     */
    private fun labelFor(
        row: RawTrack,
        kind: TrackKind,
        all: List<RawTrack>,
    ): String {
        row.title?.takeIf { it.isNotBlank() }?.let { return it }
        row.lang?.takeIf { it.isNotBlank() }?.let { return it }

        val ordinal = all.filter { it.type == row.type }.indexOfFirst { it.id == row.id } + 1
        val noun = if (kind == TrackKind.AUDIO) "Audio" else "Subtitle"
        return "$noun $ordinal"
    }

    private const val PREFIX_AUDIO = "aid"
    private const val PREFIX_SUB = "sid"
    private const val PROPERTY_AID = "aid"
    private const val PROPERTY_SID = "sid"
}
