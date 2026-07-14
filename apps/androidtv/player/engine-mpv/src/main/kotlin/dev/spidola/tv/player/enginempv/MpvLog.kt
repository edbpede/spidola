// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.player.enginempv

import android.util.Log
import dev.spidola.tv.core.playercontract.EngineError
import dev.spidola.tv.core.playercontract.PlaybackState
import dev.spidola.tv.core.playercontract.StreamRequest
import dev.spidola.tv.core.playercontract.diagnosticDetail

/**
 * This engine's logcat sink (TECH_SPEC §4.8).
 *
 * The tag matches the core's per-target scheme so one logcat filter shows the whole pipeline
 * from import to playback, and engine state transitions are logged so a support thread has a
 * coherent story.
 *
 * **The hard invariant (§12): no secret value is ever formatted into a log message.** Header
 * values and user-agent tokens are logged by *name only*, never by value, and everything mpv
 * itself says goes through [MpvLogRedaction] first — mpv logs the URL it is opening, and an
 * IPTV URL is routinely made of credentials. There is no log level at which a token becomes
 * loggable, so this file exposes no API that could take one.
 */
internal object MpvLog {
    /** Matches the core's `playback` tracing target (TECH_SPEC §4.8). */
    private const val TAG = "spidola.playback.mpv"

    /** mpv's own log lines, already redacted. Kept at debug: they are verbose and rarely read. */
    fun mpvSaid(
        prefix: String?,
        text: String,
    ) {
        if (!Log.isLoggable(TAG, Log.DEBUG)) return
        Log.d(TAG, "mpv[${prefix.orEmpty()}] ${MpvLogRedaction.redact(text.trimEnd())}")
    }

    /**
     * The request being opened — **names only**.
     *
     * The locator is not logged at all, not even redacted: [MpvLogRedaction] exists to salvage
     * diagnostic value from mpv's output, which we do not control. Here we do control it, and
     * the honest answer is that the shell already knows which channel it asked for, so the URL
     * buys nothing that would justify carrying it past this line.
     */
    fun loading(request: StreamRequest) {
        Log.i(
            TAG,
            "load: buffering=${request.buffering.name}" +
                " headers=${request.headers.joinToString(",") { it.name }.ifEmpty { "none" }}" +
                " userAgent=${if (request.userAgent != null) "overridden" else "default"}",
        )
    }

    /** An engine state transition (TECH_SPEC §4.8: transitions are logged at info/error). */
    fun transition(
        from: PlaybackState,
        to: PlaybackState,
    ) {
        val message = "state: ${from.label} -> ${to.label}"
        if (to is PlaybackState.Failed) {
            // diagnosticDetail is null for every classified variant, so this adds text only
            // for Unknown — which is exactly the case a support thread needs it for.
            Log.e(TAG, "$message (${to.error.logLabel})${to.error.diagnosticDetail?.let { ": $it" }.orEmpty()}")
        } else {
            Log.i(TAG, message)
        }
    }

    fun nativeCallFailed(
        what: String,
        code: Int,
    ) {
        Log.w(TAG, "$what failed: mpv error $code")
    }

    /**
     * The event pump was still running when [MpvClient.release] gave up waiting for it, so the
     * handle was leaked rather than destroyed under a live thread. Nothing here is recoverable;
     * it is logged because it means mpv did not return from `mpv_wait_event` after a wakeup,
     * and a support thread seeing this alongside rising memory has the whole explanation.
     */
    fun pumpOutlivedJoin(timeoutMs: Long) {
        Log.w(TAG, "release: event pump still running after ${timeoutMs}ms — leaking the mpv handle")
    }

    fun nativeLibraryMissing(error: UnsatisfiedLinkError) {
        Log.e(
            TAG,
            "libmpv is not in this build — the mpv engine is unavailable. " +
                "Build it with tools/build-libmpv-android/build.sh.",
            error,
        )
    }

    /** The failure class as a log token: stable, greppable, and not the viewer-facing copy. */
    private val EngineError.logLabel: String
        get() =
            when (this) {
                EngineError.SourceUnreachable -> "SourceUnreachable"
                EngineError.Unauthorized -> "Unauthorized"
                EngineError.UnsupportedFormat -> "UnsupportedFormat"
                EngineError.DecoderFailed -> "DecoderFailed"
                EngineError.Timeout -> "Timeout"
                is EngineError.Unknown -> "Unknown"
            }

    private val PlaybackState.label: String
        get() =
            when (this) {
                PlaybackState.Idle -> "Idle"
                PlaybackState.Loading -> "Loading"
                PlaybackState.Buffering -> "Buffering"
                PlaybackState.Playing -> "Playing"
                PlaybackState.Paused -> "Paused"
                PlaybackState.Ended -> "Ended"
                is PlaybackState.Failed -> "Failed"
            }
}
