// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.player.enginempv

import dev.spidola.tv.core.playercontract.PlaybackState

/**
 * mpv's observed property flags → the contract's non-terminal [PlaybackState] (TECH_SPEC §8).
 *
 * Pure, so the state machine the whole playback UI renders from is testable without a
 * decoder.
 *
 * **Why derive rather than react to events.** mpv has no "now playing" event. It has a set of
 * flags that are individually true or false at any moment, and the playing/paused/buffering
 * distinction is a *function* of them. Setting state directly from each event as it arrives
 * means every event handler has to know about every other flag, and they drift: the classic
 * symptom is a stream that reports Playing while the spinner is up, because a
 * `paused-for-cache` change arrived after the `pause` change that "won". Recomputing the
 * whole answer from the current flags on every change makes that unrepresentable.
 */
internal object MpvStateDerivation {
    /**
     * The mpv flags this engine observes. Names match mpv's properties exactly so the mapping
     * to `MpvEngine.observeDefaults` is one-to-one and greppable.
     */
    data class Flags(
        /** mpv has a file open (`MPV_EVENT_FILE_LOADED` has arrived for it). */
        val fileLoaded: Boolean = false,
        /** mpv's `pause` — the viewer asked to hold. */
        val pause: Boolean = false,
        /** mpv's `paused-for-cache` — the demuxer is starved. */
        val pausedForCache: Boolean = false,
        /** mpv's `core-idle` — the core is not producing frames, for any reason. */
        val coreIdle: Boolean = false,
    )

    /**
     * The current state implied by [flags].
     *
     * Never returns a terminal state: [PlaybackState.Ended] and [PlaybackState.Failed] come
     * from `MPV_EVENT_END_FILE` (via [MpvErrorMapping]) because only that event distinguishes
     * "the stream stopped" from "the stream is momentarily quiet", and no combination of
     * these flags does.
     */
    fun stateFor(flags: Flags): PlaybackState =
        when {
            // Before FILE_LOADED every other flag is noise: core-idle is trivially true while
            // opening, so checking it first would report Buffering for the whole of the
            // click-to-first-frame window that PRD §9 measures as Loading.
            !flags.fileLoaded -> PlaybackState.Loading

            // An explicit pause outranks starvation. Both can be true at once — pausing a
            // starved stream is normal — and the viewer who pressed pause should see Paused,
            // not a spinner suggesting the app is still trying.
            flags.pause -> PlaybackState.Paused

            flags.pausedForCache -> PlaybackState.Buffering

            // core-idle without paused-for-cache: seeking, or a VO that has not produced its
            // first frame yet. Buffering is the honest report — video is not advancing.
            flags.coreIdle -> PlaybackState.Buffering

            else -> PlaybackState.Playing
        }

    /**
     * Reads an mpv flag property.
     *
     * mpv renders flags as the literal strings `yes`/`no` when read as `MPV_FORMAT_STRING`,
     * which is how this engine observes them (see `MpvClient.Format.STRING`). Anything else —
     * including the `null` mpv returns for a property that is not currently available — is
     * false, because "mpv would not tell us" is never a reason to claim a flag is set.
     */
    fun flagOf(value: String?): Boolean = value == "yes"
}
