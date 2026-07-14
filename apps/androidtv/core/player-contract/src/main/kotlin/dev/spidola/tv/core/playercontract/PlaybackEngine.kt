// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.core.playercontract

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import kotlinx.coroutines.flow.StateFlow

/**
 * An engine's stable identity, used as the persisted override key and the registry key.
 *
 * Deliberately an opaque string rather than a contract-side enum: the core already persists
 * `preferred_engine` as an opaque key precisely so engine identities stay a shell concept
 * (TECH_SPEC §8). A closed enum here would force the contract to know its own implementors,
 * inverting the dependency — the composition root registers engines, the contract never enumerates
 * them.
 */
@JvmInline
value class EngineId(
    val value: String,
) {
    companion object {
        /**
         * The libmpv engine — mpv-class codec breadth; the Android fallback (TECH_SPEC §8). Shares
         * its key with the tvOS MPVKit engine: same engine concept, so a per-channel override set
         * on one platform reads correctly on the other.
         */
        val MPV = EngineId("mpv")

        /** The Media3/ExoPlayer engine — the Android default (TECH_SPEC §8). */
        val EXOPLAYER = EngineId("exoplayer")
    }
}

/**
 * The playback engine contract (TECH_SPEC §8). Both platforms implement the same conceptual
 * interface so product behaviour is identical.
 *
 * Engines are **disposable and cheap to re-create**, because zapping destroys and rebuilds them
 * constantly — the zap path is the performance-critical consumer and its budget is an acceptance
 * test per engine. Implementations therefore keep construction free of I/O: [load] is where work
 * starts.
 *
 * Engines are main-thread-affine: every implementation wraps a player whose lifecycle must be
 * driven from the main thread. Callers are the playback slice's composables and view model, which
 * are already main-safe; engine internals that genuinely run off-main (mpv's event loop) hop back
 * explicitly.
 */
interface PlaybackEngine {
    /** This engine's stable identity — the value persisted by an override and shown in diagnostics. */
    val id: EngineId

    /**
     * The read-only state machine. The shell's single source of playback truth. Hot and
     * conflated: a late collector always observes the current state, so it cannot miss a terminal
     * [PlaybackState.Failed].
     */
    val state: StateFlow<PlaybackState>

    /**
     * The current track menu. Populated once the stream's tracks are known, so it is empty in
     * [PlaybackState.Loading] and meaningful from [PlaybackState.Playing] onward.
     */
    val tracks: StateFlow<TrackSelection>

    /**
     * Whether this stream can seek. Live streams generally cannot; the UI hides the scrubber rather
     * than offering a control that does nothing.
     */
    val isSeekable: StateFlow<Boolean>

    /** The video surface this engine renders into, for the playback screen to host. */
    @Composable
    fun Surface(modifier: Modifier)

    /**
     * Opens [request] and begins playback. Returns immediately; progress arrives via [state].
     * Calling [load] twice on one engine is not supported — dispose and rebuild instead, which is
     * exactly what the zap path does.
     */
    fun load(request: StreamRequest)

    fun play()

    fun pause()

    /**
     * Seeks to [seconds] from the start. A no-op when [isSeekable] is false, so the caller need not
     * guard.
     */
    fun seekTo(seconds: Double)

    fun select(track: TrackId)

    /** Turns subtitles off. Distinct from [select] because "no subtitle" is not a track. */
    fun clearSubtitle()

    fun setAspect(mode: AspectMode)

    /**
     * Tears the engine down and releases its decoder. Idempotent: the shell calls it from
     * `DisposableEffect.onDispose` and on the terminal-state path, and neither knows whether the
     * other ran.
     */
    fun release()
}
