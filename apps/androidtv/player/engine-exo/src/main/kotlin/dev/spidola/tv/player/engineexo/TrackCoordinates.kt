// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.player.engineexo

import dev.spidola.tv.core.playercontract.TrackId

/**
 * Where a track lives in ExoPlayer's track model: `groupIndex` indexes `Player.currentTracks.groups`,
 * `trackIndex` indexes the formats within that group.
 *
 * ExoPlayer needs both numbers to build a `TrackSelectionOverride`, but the contract's [TrackId] is
 * one opaque string — deliberately, so the UI never learns any engine's numbering (MediaTrack.kt).
 * This type is the seam: the pair is encoded on the way out and decoded on the way back in.
 */
internal data class TrackCoordinates(
    val groupIndex: Int,
    val trackIndex: Int,
)

/**
 * The encoding: `"<groupIndex>:<trackIndex>"`, e.g. `"3:0"`.
 *
 * Indices are only meaningful for the tracks that produced them — a new stream renumbers the
 * groups, which is why the engine republishes the whole track menu on every `onTracksChanged`
 * rather than letting a stale id survive a load. Decoding validates rather than trusts, because
 * an id can reach [select] from UI state that a track change has already invalidated.
 */
internal fun TrackCoordinates.encode(): TrackId = TrackId("$groupIndex$TRACK_ID_SEPARATOR$trackIndex")

/** Decodes an id produced by [encode]. Null when the id is malformed or carries negative indices. */
internal fun TrackId.decode(): TrackCoordinates? {
    val parts = value.split(TRACK_ID_SEPARATOR)
    if (parts.size != TRACK_ID_PART_COUNT) return null

    val groupIndex = parts[0].toIntOrNull()
    val trackIndex = parts[1].toIntOrNull()
    val valid = groupIndex != null && trackIndex != null && groupIndex >= 0 && trackIndex >= 0
    return if (valid) TrackCoordinates(groupIndex, trackIndex) else null
}

private const val TRACK_ID_SEPARATOR = ":"
private const val TRACK_ID_PART_COUNT = 2
