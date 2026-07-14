// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

@file:OptIn(markerClass = [UnstableApi::class])

package dev.spidola.tv.player.engineexo

import androidx.annotation.OptIn
import androidx.media3.common.PlaybackException
import androidx.media3.common.util.UnstableApi
import androidx.media3.datasource.HttpDataSource.InvalidResponseCodeException
import dev.spidola.tv.core.playercontract.EngineError
import java.net.HttpURLConnection.HTTP_FORBIDDEN
import java.net.HttpURLConnection.HTTP_UNAUTHORIZED

/**
 * Translates ExoPlayer's ~50 error codes into the six-class shared taxonomy (TECH_SPEC §8).
 *
 * This mapping is the engine's most product-visible logic, because the class it picks decides what
 * the viewer is offered: only [EngineError.UnsupportedFormat] and [EngineError.DecoderFailed] earn
 * the "Try other player" button, so misfiling a network failure as a format failure sends the
 * viewer to a second engine that will fail identically.
 *
 * A pure function over the exception, deliberately: it is the one part of the engine that can be
 * exhaustively tested on the JVM with no device and no decoder.
 */
internal fun PlaybackException.toEngineError(): EngineError {
    // Checked before the code table because the code alone cannot tell "the server refused us"
    // from "the server is broken" — both surface as ERROR_CODE_IO_BAD_HTTP_STATUS.
    val responseCode = findHttpResponseCode()
    if (responseCode == HTTP_UNAUTHORIZED || responseCode == HTTP_FORBIDDEN) {
        return EngineError.Unauthorized
    }

    return when (errorCode) {
        PlaybackException.ERROR_CODE_TIMEOUT,
        PlaybackException.ERROR_CODE_IO_NETWORK_CONNECTION_TIMEOUT,
        -> EngineError.Timeout

        // A source that answers HTML where video was asked for is overwhelmingly a login wall or a
        // captive portal — an auth failure wearing a content-type error's clothes.
        PlaybackException.ERROR_CODE_IO_INVALID_HTTP_CONTENT_TYPE,
        PlaybackException.ERROR_CODE_IO_NO_PERMISSION,
        -> EngineError.Unauthorized

        PlaybackException.ERROR_CODE_IO_UNSPECIFIED,
        PlaybackException.ERROR_CODE_IO_NETWORK_CONNECTION_FAILED,
        PlaybackException.ERROR_CODE_IO_BAD_HTTP_STATUS,
        PlaybackException.ERROR_CODE_IO_FILE_NOT_FOUND,
        PlaybackException.ERROR_CODE_IO_CLEARTEXT_NOT_PERMITTED,
        PlaybackException.ERROR_CODE_IO_READ_POSITION_OUT_OF_RANGE,
        -> EngineError.SourceUnreachable

        PlaybackException.ERROR_CODE_PARSING_CONTAINER_MALFORMED,
        PlaybackException.ERROR_CODE_PARSING_MANIFEST_MALFORMED,
        PlaybackException.ERROR_CODE_PARSING_CONTAINER_UNSUPPORTED,
        PlaybackException.ERROR_CODE_PARSING_MANIFEST_UNSUPPORTED,
        PlaybackException.ERROR_CODE_NOT_SUPPORTED,
        -> EngineError.UnsupportedFormat

        PlaybackException.ERROR_CODE_DECODER_INIT_FAILED,
        PlaybackException.ERROR_CODE_DECODER_QUERY_FAILED,
        PlaybackException.ERROR_CODE_DECODING_FAILED,
        PlaybackException.ERROR_CODE_DECODING_FORMAT_EXCEEDS_CAPABILITIES,
        PlaybackException.ERROR_CODE_DECODING_FORMAT_UNSUPPORTED,
        PlaybackException.ERROR_CODE_DECODING_RESOURCES_RECLAIMED,
        -> EngineError.DecoderFailed

        else -> EngineError.Unknown(diagnostic())
    }
}

/**
 * The diagnostic string carried by [EngineError.Unknown], bound for the log stream.
 *
 * Built from ExoPlayer's code name and the throwing class only — never from [PlaybackException.message],
 * which embeds the failing `DataSpec` and therefore the stream URL. Xtream locators carry the
 * subscriber's username and password in the path, so interpolating the message here would write
 * credentials into logcat and into every exported log file (TECH_SPEC §12).
 */
private fun PlaybackException.diagnostic(): String {
    val origin = (cause ?: this)::class.java.simpleName
    return "exoplayer $errorCodeName (code $errorCode) from $origin"
}

/**
 * The HTTP status behind this failure, if one is. ExoPlayer wraps the response-code exception
 * several causes deep, so the whole chain is walked; the visited set guards the self-referential
 * chains some HTTP stacks produce.
 */
private fun PlaybackException.findHttpResponseCode(): Int? {
    val visited = mutableSetOf<Throwable>()
    var current: Throwable? = this
    while (current != null && visited.add(current)) {
        if (current is InvalidResponseCodeException) return current.responseCode
        current = current.cause
    }
    return null
}
