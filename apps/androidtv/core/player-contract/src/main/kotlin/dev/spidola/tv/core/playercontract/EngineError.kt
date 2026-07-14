// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.core.playercontract

/**
 * The shared engine failure taxonomy (TECH_SPEC §8). Exactly the classes the PRD's error UX needs
 * — no more, so every engine maps its native failures onto a set the playback UI can present
 * without knowing which engine produced it.
 *
 * The Swift mirror is `PlayerContract.EngineError`; the two must stay variant-for-variant
 * identical, since parity is the point of specifying the contract at all. A sealed hierarchy so
 * `when` is exhaustive and adding a class breaks every consumer that must decide about it.
 */
sealed interface EngineError {
    /** The stream's host could not be reached (DNS, connection refused, network down). */
    data object SourceUnreachable : EngineError

    /** The stream's host rejected our credentials (HTTP 401/403). */
    data object Unauthorized : EngineError

    /** The container or protocol is one this engine cannot demux at all. */
    data object UnsupportedFormat : EngineError

    /** The container demuxed, but a codec inside it failed to decode. */
    data object DecoderFailed : EngineError

    /** The stream neither opened nor failed within the engine's deadline. */
    data object Timeout : EngineError

    /**
     * Anything the engine could not classify. [detail] is diagnostic text for the log stream —
     * never for the screen (PRD §8.6).
     */
    data class Unknown(
        val detail: String,
    ) : EngineError
}

/**
 * Whether this failure should offer the one-button "Try other player" (TECH_SPEC §8).
 *
 * Only a format or decode failure means "a different engine could plausibly play this". A network
 * or auth failure would fail identically on any engine, so offering another player there would be
 * a lie that wastes the viewer's time.
 *
 * Fallback is **loud, never silent**: this only decides whether the button is offered, never
 * whether an engine is swapped behind the viewer's back — a silent swap would make engine bugs
 * invisible and support impossible.
 */
val EngineError.offersOtherPlayer: Boolean
    get() =
        when (this) {
            EngineError.UnsupportedFormat, EngineError.DecoderFailed -> true
            EngineError.SourceUnreachable, EngineError.Unauthorized, EngineError.Timeout -> false
            is EngineError.Unknown -> false
        }

/** The couch-legible failure class (PRD §6.3, §8.6 voice). No system jargon, no engine names. */
val EngineError.failureClass: String
    get() =
        when (this) {
            EngineError.SourceUnreachable -> "Can't reach this channel"
            EngineError.Unauthorized -> "This channel refused the login"
            EngineError.UnsupportedFormat -> "This channel's format isn't supported"
            EngineError.DecoderFailed -> "This channel wouldn't play"
            EngineError.Timeout -> "This channel is taking too long"
            is EngineError.Unknown -> "This channel wouldn't play"
        }

/** A one-sentence, jargon-free explanation of what happened. */
val EngineError.message: String
    get() =
        when (this) {
            EngineError.SourceUnreachable -> "The stream's server didn't answer."
            EngineError.Unauthorized -> "The stream's server didn't accept this source's login."
            EngineError.UnsupportedFormat -> "The other player may handle this format."
            EngineError.DecoderFailed -> "The video started but couldn't be decoded."
            EngineError.Timeout -> "The stream didn't start in time."
            is EngineError.Unknown -> "Something went wrong starting this channel."
        }

/**
 * Diagnostic detail for the log stream only — `null` for every classified variant, since a
 * classified failure's diagnosis is its class.
 */
val EngineError.diagnosticDetail: String?
    get() =
        when (this) {
            is EngineError.Unknown -> detail
            EngineError.SourceUnreachable, EngineError.Unauthorized, EngineError.UnsupportedFormat,
            EngineError.DecoderFailed, EngineError.Timeout,
            -> null
        }
