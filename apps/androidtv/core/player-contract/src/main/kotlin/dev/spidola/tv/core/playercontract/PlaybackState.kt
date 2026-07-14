// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.core.playercontract

/**
 * The small state machine every engine emits (TECH_SPEC §8). The playback UI matches this
 * exhaustively and knows nothing about which engine produced it — that is what makes the "Try
 * other player" swap a re-render rather than a rewrite.
 *
 * The Swift mirror is `PlayerContract.PlaybackState`.
 */
sealed interface PlaybackState {
    /** Constructed, nothing loaded. */
    data object Idle : PlaybackState

    /**
     * Opening the stream: the window between `load` and the first decoded frame. The
     * click-to-first-frame budget (PRD §9) is measured across exactly this state.
     */
    data object Loading : PlaybackState

    /** Playing, but starved — video is not advancing. */
    data object Buffering : PlaybackState

    /** Video is advancing. */
    data object Playing : PlaybackState

    /** Loaded and holding position. */
    data object Paused : PlaybackState

    /**
     * The stream ended on its own (VOD run-out; a live stream reaching this means the origin
     * closed it).
     */
    data object Ended : PlaybackState

    /**
     * Terminal failure. The engine is spent; the shell disposes it and either offers another
     * player or presents the error.
     */
    data class Failed(
        val error: EngineError,
    ) : PlaybackState
}

/** Whether video is on screen. Drives whether the shell may hide its loading treatment. */
val PlaybackState.isShowingVideo: Boolean
    get() =
        when (this) {
            PlaybackState.Playing, PlaybackState.Paused, PlaybackState.Buffering -> true
            PlaybackState.Idle, PlaybackState.Loading, PlaybackState.Ended -> false
            is PlaybackState.Failed -> false
        }

/** Whether the engine has reached a terminal state and should be disposed. */
val PlaybackState.isTerminal: Boolean
    get() =
        when (this) {
            PlaybackState.Ended -> true
            is PlaybackState.Failed -> true
            PlaybackState.Idle, PlaybackState.Loading, PlaybackState.Buffering,
            PlaybackState.Playing, PlaybackState.Paused,
            -> false
        }

/**
 * The failure this state carries, if it is a failure. Keeps the fallback decision to one line at
 * the call site rather than a re-match.
 */
val PlaybackState.failure: EngineError?
    get() =
        when (this) {
            is PlaybackState.Failed -> error
            PlaybackState.Idle, PlaybackState.Loading, PlaybackState.Buffering,
            PlaybackState.Playing, PlaybackState.Paused, PlaybackState.Ended,
            -> null
        }
