// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.player.engineexo

import androidx.media3.common.C
import androidx.media3.common.Format
import androidx.media3.common.Tracks
import dev.spidola.tv.core.playercontract.MediaTrack
import dev.spidola.tv.core.playercontract.TrackId
import dev.spidola.tv.core.playercontract.TrackKind
import dev.spidola.tv.core.playercontract.TrackSelection
import kotlinx.collections.immutable.toImmutableList
import java.util.Locale

/**
 * Projects ExoPlayer's track model onto the contract's track menu (MediaTrack.kt).
 *
 * Video groups are skipped: the contract has no video [TrackKind] because no shipped surface picks
 * a video rendition, and unsupported tracks are skipped because offering a track this device cannot
 * decode is offering a failure.
 */
internal fun Tracks.toTrackSelection(): TrackSelection {
    val available = mutableListOf<MediaTrack>()
    var selectedAudio: TrackId? = null
    var selectedSubtitle: TrackId? = null

    groups.forEachIndexed { groupIndex, group ->
        val kind = group.type.toTrackKind() ?: return@forEachIndexed
        for (trackIndex in 0 until group.length) {
            if (!group.isTrackSupported(trackIndex)) continue
            val id = TrackCoordinates(groupIndex, trackIndex).encode()
            val format = group.getTrackFormat(trackIndex)
            available +=
                MediaTrack(
                    id = id,
                    kind = kind,
                    label = format.readableLabel(kind, available.count { it.kind == kind }),
                    language = format.language,
                )
            if (group.isTrackSelected(trackIndex)) {
                when (kind) {
                    TrackKind.AUDIO -> selectedAudio = id
                    TrackKind.SUBTITLE -> selectedSubtitle = id
                }
            }
        }
    }

    return TrackSelection(
        available = available.toImmutableList(),
        selectedAudio = selectedAudio,
        selectedSubtitle = selectedSubtitle,
    )
}

private fun Int.toTrackKind(): TrackKind? =
    when (this) {
        C.TRACK_TYPE_AUDIO -> TrackKind.AUDIO
        C.TRACK_TYPE_TEXT -> TrackKind.SUBTITLE
        else -> null
    }

/**
 * The label the track menu shows. The contract asks for the engine's label "already de-jargoned
 * where the engine gives us the chance": a stream's own label wins, otherwise the language tag is
 * resolved to its display name so the menu offers "English" rather than the wire's `en`. Streams
 * that declare neither fall back to a position, which is at least honest about being unnamed.
 */
private fun Format.readableLabel(
    kind: TrackKind,
    ordinal: Int,
): String =
    label?.takeIf { it.isNotBlank() }
        ?: displayLanguage()
        ?: kind.positionalLabel(ordinal)

/** The stream's language tag as a viewer would say it: `en` becomes "English". */
private fun Format.displayLanguage(): String? =
    language
        ?.takeIf { it.isNotBlank() && it != C.LANGUAGE_UNDETERMINED }
        ?.let { Locale.forLanguageTag(it).displayLanguage }
        ?.takeIf { it.isNotBlank() }

private fun TrackKind.positionalLabel(ordinal: Int): String =
    when (this) {
        TrackKind.AUDIO -> "Audio ${ordinal + 1}"
        TrackKind.SUBTITLE -> "Subtitle ${ordinal + 1}"
    }
