// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.player.enginempv

import dev.spidola.tv.core.playercontract.EngineError

/**
 * mpv's failure vocabulary → the contract's [EngineError] taxonomy (TECH_SPEC §8).
 *
 * Pure, and deliberately free of any native reference so it is unit-testable on the JVM with
 * no `libmpv.so` present — which matters, because this mapping decides whether the viewer is
 * offered "Try other player", and that decision must be tested over every branch rather than
 * on a device with one broken stream.
 *
 * **Why the log text is an input.** mpv's own error codes cannot distinguish the cases the
 * PRD's error UX must distinguish. Every one of "DNS failed", "connection refused",
 * "HTTP 401", and "HTTP 403" arrives as the single code `MPV_ERROR_LOADING_FAILED` (-13).
 * The only place that distinction survives is FFmpeg's log line. So classification consults
 * the diagnostic text first and the code second — not because text-matching is pleasant, but
 * because the alternative is collapsing four different viewer-facing outcomes into one
 * unhelpful "couldn't play".
 *
 * The diagnostic passed in **must already be redacted** ([MpvLogRedaction]); this file puts
 * it into [EngineError.Unknown.detail], which reaches the log stream.
 */
internal object MpvErrorMapping {
    /**
     * mpv's error codes, from `client.h`. Only the ones this mapping actually decides on are
     * named — an unnamed code lands in the `else` arm and becomes [EngineError.Unknown],
     * which is the honest answer for a code we have not reasoned about.
     */
    object Code {
        const val SUCCESS = 0
        const val LOADING_FAILED = -13
        const val AO_INIT_FAILED = -14
        const val VO_INIT_FAILED = -15
        const val NOTHING_TO_PLAY = -16
        const val UNKNOWN_FORMAT = -17
        const val UNSUPPORTED = -18
        const val GENERIC = -20
    }

    /** `MPV_END_FILE_REASON_*`, from `client.h`. */
    object EndFileReason {
        const val EOF = 0
        const val STOP = 2
        const val QUIT = 3
        const val ERROR = 4
        const val REDIRECT = 5
    }

    // Matched against the redacted diagnostic. Ordered most-specific-first by the caller.
    private val UNAUTHORIZED = Regex("""\b(401|403)\b|\bunauthorized\b|\bforbidden\b""", RegexOption.IGNORE_CASE)
    private val UNREACHABLE =
        Regex(
            """\b(name or service not known|temporary failure in name resolution|no such host|""" +
                """could not resolve|failed to resolve|connection refused|network is unreachable|""" +
                """no route to host|host is unreachable|connection reset)\b""",
            RegexOption.IGNORE_CASE,
        )
    private val TIMEOUT =
        Regex("""\b(timed out|timeout|connection timed out|operation timed out)\b""", RegexOption.IGNORE_CASE)
    private val UNSUPPORTED_FORMAT =
        Regex(
            """\b(invalid data found when processing input|unknown format|could not (find|open) codec|""" +
                """no decoder for|demux(ing|er)? fail|failed to (recognize|open) file format|""" +
                """unsupported|could not demux)\b""",
            RegexOption.IGNORE_CASE,
        )
    private val DECODE_FAILED =
        Regex(
            """\b(error while decoding|decoder error|failed to decode|hardware decoding failed|""" +
                """could not initialize (video|audio) (chain|decoder)|error initializing decoder|""" +
                """decode_slice_header error)\b""",
            RegexOption.IGNORE_CASE,
        )

    /**
     * Classifies an `MPV_EVENT_END_FILE`.
     *
     * Returns `null` when the file ended for a reason that is not a failure, so the caller
     * does not have to re-derive "was this actually bad": `EOF` is the stream running out
     * (the contract's `Ended`), and `STOP`/`QUIT`/`REDIRECT` are our own teardown or mpv
     * following a redirect — none of which the viewer should ever see an error for.
     */
    fun endFileError(
        reason: Int,
        errorCode: Int,
        diagnostic: String?,
    ): EngineError? =
        when (reason) {
            EndFileReason.ERROR -> engineErrorFor(errorCode, diagnostic)
            EndFileReason.EOF ->
                classifyDiagnostic(diagnostic)
                    ?.takeIf { it == EngineError.DecoderFailed }
            EndFileReason.STOP, EndFileReason.QUIT, EndFileReason.REDIRECT -> null
            // A reason mpv grew after this was written. Treating it as a failure would invent
            // an error the viewer cannot act on; ignoring it keeps the state machine honest.
            else -> null
        }

    /**
     * Classifies an mpv error code plus whatever mpv said about it.
     *
     * [diagnostic] is consulted before [errorCode] because it is strictly more specific: the
     * code says "loading failed", the text says why.
     */
    fun engineErrorFor(
        errorCode: Int,
        diagnostic: String?,
    ): EngineError {
        classifyDiagnostic(diagnostic)?.let { return it }

        return when (errorCode) {
            // The container or protocol was never openable.
            Code.UNKNOWN_FORMAT, Code.NOTHING_TO_PLAY, Code.UNSUPPORTED -> EngineError.UnsupportedFormat
            // The chain came up but a decoder or output could not initialize. Offering the
            // other engine here is honest: a different decoder genuinely might succeed.
            Code.VO_INIT_FAILED, Code.AO_INIT_FAILED -> EngineError.DecoderFailed
            // -13 with nothing quotable in the log. It is overwhelmingly a network failure,
            // but "overwhelmingly" is not "certainly", and guessing SourceUnreachable here
            // would suppress the "Try other player" button for a stream another engine could
            // have played. Unknown keeps the guess out of the viewer's way.
            Code.LOADING_FAILED -> EngineError.Unknown(detail(errorCode, diagnostic))
            Code.GENERIC -> EngineError.Unknown(detail(errorCode, diagnostic))
            else -> EngineError.Unknown(detail(errorCode, diagnostic))
        }
    }

    private fun classifyDiagnostic(diagnostic: String?): EngineError? {
        val text = diagnostic?.takeIf { it.isNotBlank() } ?: return null
        return when {
            // Auth before reachability: a 401 body arrives over a connection that plainly
            // worked, so a "connection reset" later in the same log must not outrank it.
            UNAUTHORIZED.containsMatchIn(text) -> EngineError.Unauthorized
            TIMEOUT.containsMatchIn(text) -> EngineError.Timeout
            UNREACHABLE.containsMatchIn(text) -> EngineError.SourceUnreachable
            // Decode before format: "error while decoding" implies the demuxer already
            // succeeded, so it is the more specific claim.
            DECODE_FAILED.containsMatchIn(text) -> EngineError.DecoderFailed
            UNSUPPORTED_FORMAT.containsMatchIn(text) -> EngineError.UnsupportedFormat
            else -> null
        }
    }

    /** Diagnostic text for the log stream (PRD §8.6: never for the screen). */
    private fun detail(
        errorCode: Int,
        diagnostic: String?,
    ): String =
        diagnostic
            ?.takeIf { it.isNotBlank() }
            ?.let { "mpv error $errorCode: $it" }
            ?: "mpv error $errorCode"
}
