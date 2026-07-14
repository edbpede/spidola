// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.core.playercontract

import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.persistentListOf

/**
 * An engine-scoped track handle. A value class rather than a bare `String` so a subtitle id can
 * never be passed where an audio id belongs, and so each engine keeps its own numbering (mpv track
 * ids, ExoPlayer group indices) without leaking it into the UI.
 */
@JvmInline
value class TrackId(
    val value: String,
)

/**
 * Which stream a track belongs to. Video tracks are deliberately absent: no shipped surface selects
 * one, and an unused arm would be an untested arm.
 */
enum class TrackKind { AUDIO, SUBTITLE }

/** One selectable audio or subtitle track, as the UI shows it. */
data class MediaTrack(
    val id: TrackId,
    val kind: TrackKind,
    /** The engine's human label, already de-jargoned where the engine gives us the chance. */
    val label: String,
    /** BCP-47-ish language tag when the stream declares one. */
    val language: String? = null,
)

/**
 * The engine's current track menu: what exists and what is on. Selection is a separate field rather
 * than an `isSelected` flag per track, so "exactly one audio track is selected" is true by
 * construction instead of by convention.
 */
data class TrackSelection(
    val available: ImmutableList<MediaTrack> = persistentListOf(),
    val selectedAudio: TrackId? = null,
    val selectedSubtitle: TrackId? = null,
) {
    fun tracksOf(kind: TrackKind): List<MediaTrack> = available.filter { it.kind == kind }
}
