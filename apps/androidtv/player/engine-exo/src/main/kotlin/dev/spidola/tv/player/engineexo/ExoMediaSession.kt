// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.player.engineexo

import android.content.Context
import android.util.Log
import androidx.media3.common.Player
import androidx.media3.session.MediaSession
import java.util.concurrent.atomic.AtomicLong

/**
 * Publishes [player] to the platform media session so the TV remote's transport keys and the
 * assistant's "pause" reach playback (TECH_SPEC §7). Without a session those inputs go nowhere:
 * the system has no other handle on an app's player.
 *
 * The id is unique per session because zapping overlaps engines — the outgoing engine may not have
 * released before the incoming one builds, and `MediaSession` rejects a duplicate id by throwing.
 *
 * A session that cannot be built costs the remote, not the picture, so it is logged and skipped
 * rather than allowed to take playback down with it.
 */
internal fun openMediaSession(
    context: Context,
    player: Player,
): MediaSession? =
    runCatching {
        MediaSession.Builder(context, player).setId(nextSessionId()).build()
    }.onFailure { failure ->
        Log.w(PLAYBACK_TAG, "exoplayer: media session unavailable; remote transport keys are inert", failure)
    }.getOrNull()

private val sessionCounter = AtomicLong()

private fun nextSessionId(): String = "spidola-exo-${sessionCounter.incrementAndGet()}"
