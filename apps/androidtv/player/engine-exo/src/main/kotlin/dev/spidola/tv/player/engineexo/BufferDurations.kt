// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

@file:OptIn(markerClass = [UnstableApi::class])

package dev.spidola.tv.player.engineexo

import androidx.annotation.OptIn
import androidx.media3.common.util.UnstableApi
import androidx.media3.exoplayer.DefaultLoadControl
import androidx.media3.exoplayer.LoadControl
import dev.spidola.tv.core.playercontract.BufferingProfile

/**
 * ExoPlayer's four buffer thresholds, in milliseconds, split out from [LoadControl] so the numbers
 * behind each [BufferingProfile] are data a JVM test can assert rather than state sealed inside a
 * built player.
 *
 * `DefaultLoadControl` rejects a set where `bufferForPlaybackMs` or
 * `bufferForPlaybackAfterRebufferMs` exceeds `minBufferMs`, or where `minBufferMs` exceeds
 * `maxBufferMs` — the profiles below hold that ordering.
 */
internal data class BufferDurations(
    /** Steady-state floor: below this, the loader keeps fetching. */
    val minBufferMs: Int,
    /** Steady-state ceiling: the loader stops here. */
    val maxBufferMs: Int,
    /** How much media must be buffered before playback starts — the direct cost of a zap. */
    val bufferForPlaybackMs: Int,
    /** How much must be re-buffered before playback resumes after starving. */
    val bufferForPlaybackAfterRebufferMs: Int,
)

/**
 * Fastest start. `bufferForPlaybackMs` is the profile's whole point: ExoPlayer's own default of
 * 2500 ms would spend the PRD's entire two-second click-to-first-frame budget before a frame is
 * decoded. 500 ms is ~12 frames at 25 fps — enough for the decoder to start cleanly while leaving
 * the rest of the budget for DNS, connect, and manifest fetch. The small cushion is the trade: a
 * jittery source starves quickly at this profile.
 */
private val LOW_DURATIONS =
    BufferDurations(
        minBufferMs = 5_000,
        maxBufferMs = 10_000,
        bufferForPlaybackMs = 500,
        bufferForPlaybackAfterRebufferMs = 1_000,
    )

/**
 * The default trade. Start still fits the two-second budget on a healthy source, and the 15–30 s
 * cushion absorbs the re-buffering a typical IPTV origin produces.
 */
private val BALANCED_DURATIONS =
    BufferDurations(
        minBufferMs = 15_000,
        maxBufferMs = 30_000,
        bufferForPlaybackMs = 1_000,
        bufferForPlaybackAfterRebufferMs = 2_500,
    )

/**
 * Smoothest playback. Deliberately forfeits the zap budget — 2500 ms of pre-roll alone spends it —
 * in exchange for a minute of cushion, which is what rides out an origin that stalls for seconds at
 * a time. This is the profile for a viewer who has already decided that stutter costs more than
 * start-up delay.
 */
private val GENEROUS_DURATIONS =
    BufferDurations(
        minBufferMs = 30_000,
        maxBufferMs = 60_000,
        bufferForPlaybackMs = 2_500,
        bufferForPlaybackAfterRebufferMs = 5_000,
    )

internal val BufferingProfile.durations: BufferDurations
    get() =
        when (this) {
            BufferingProfile.LOW -> LOW_DURATIONS
            BufferingProfile.BALANCED -> BALANCED_DURATIONS
            BufferingProfile.GENEROUS -> GENEROUS_DURATIONS
        }

internal fun BufferingProfile.toLoadControl(): LoadControl =
    with(durations) {
        DefaultLoadControl.Builder()
            .setBufferDurationsMs(
                minBufferMs,
                maxBufferMs,
                bufferForPlaybackMs,
                bufferForPlaybackAfterRebufferMs,
            ).build()
    }
