// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

@file:OptIn(markerClass = [UnstableApi::class])

package dev.spidola.tv.player.engineexo

import androidx.annotation.OptIn
import androidx.media3.common.PlaybackException
import androidx.media3.common.util.UnstableApi
import androidx.media3.datasource.DataSpec
import androidx.media3.datasource.HttpDataSource.InvalidResponseCodeException
import dev.spidola.tv.core.playercontract.EngineError
import dev.spidola.tv.core.playercontract.offersOtherPlayer
import io.mockk.mockk
import org.junit.jupiter.api.Test
import java.io.IOException
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNotNull
import kotlin.test.assertTrue

/**
 * The error mapping is the engine's highest-consequence logic — the class it returns decides
 * whether the viewer is offered a second engine — and it is pure, so every branch is asserted here
 * rather than hoped for on a device.
 */
class ExoErrorMappingTest {
    @Test
    fun `network and io failures are unreachable`() {
        val unreachable =
            listOf(
                PlaybackException.ERROR_CODE_IO_UNSPECIFIED,
                PlaybackException.ERROR_CODE_IO_NETWORK_CONNECTION_FAILED,
                PlaybackException.ERROR_CODE_IO_BAD_HTTP_STATUS,
                PlaybackException.ERROR_CODE_IO_FILE_NOT_FOUND,
                PlaybackException.ERROR_CODE_IO_CLEARTEXT_NOT_PERMITTED,
                PlaybackException.ERROR_CODE_IO_READ_POSITION_OUT_OF_RANGE,
            )
        unreachable.forEach { code ->
            assertEquals(EngineError.SourceUnreachable, playbackException(code).toEngineError(), "code $code")
        }
    }

    @Test
    fun `content-type and permission failures are unauthorized`() {
        val unauthorized =
            listOf(
                PlaybackException.ERROR_CODE_IO_INVALID_HTTP_CONTENT_TYPE,
                PlaybackException.ERROR_CODE_IO_NO_PERMISSION,
            )
        unauthorized.forEach { code ->
            assertEquals(EngineError.Unauthorized, playbackException(code).toEngineError(), "code $code")
        }
    }

    @Test
    fun `parsing failures are unsupported format`() {
        val unsupported =
            listOf(
                PlaybackException.ERROR_CODE_PARSING_CONTAINER_MALFORMED,
                PlaybackException.ERROR_CODE_PARSING_MANIFEST_MALFORMED,
                PlaybackException.ERROR_CODE_PARSING_CONTAINER_UNSUPPORTED,
                PlaybackException.ERROR_CODE_PARSING_MANIFEST_UNSUPPORTED,
                PlaybackException.ERROR_CODE_NOT_SUPPORTED,
            )
        unsupported.forEach { code ->
            assertEquals(EngineError.UnsupportedFormat, playbackException(code).toEngineError(), "code $code")
        }
    }

    @Test
    fun `decoder failures are decoder failed`() {
        val decoder =
            listOf(
                PlaybackException.ERROR_CODE_DECODER_INIT_FAILED,
                PlaybackException.ERROR_CODE_DECODER_QUERY_FAILED,
                PlaybackException.ERROR_CODE_DECODING_FAILED,
                PlaybackException.ERROR_CODE_DECODING_FORMAT_EXCEEDS_CAPABILITIES,
                PlaybackException.ERROR_CODE_DECODING_FORMAT_UNSUPPORTED,
                PlaybackException.ERROR_CODE_DECODING_RESOURCES_RECLAIMED,
            )
        decoder.forEach { code ->
            assertEquals(EngineError.DecoderFailed, playbackException(code).toEngineError(), "code $code")
        }
    }

    @Test
    fun `timeouts are timeout`() {
        val timeouts =
            listOf(
                PlaybackException.ERROR_CODE_TIMEOUT,
                PlaybackException.ERROR_CODE_IO_NETWORK_CONNECTION_TIMEOUT,
            )
        timeouts.forEach { code ->
            assertEquals(EngineError.Timeout, playbackException(code).toEngineError(), "code $code")
        }
    }

    @Test
    fun `unclassified codes are unknown carrying the code name`() {
        val error = playbackException(PlaybackException.ERROR_CODE_DRM_LICENSE_EXPIRED).toEngineError()

        val unknown = assertIsUnknown(error)
        assertTrue(
            unknown.detail.contains("ERROR_CODE_DRM_LICENSE_EXPIRED"),
            "detail should name the ExoPlayer code, was '${unknown.detail}'",
        )
    }

    @Test
    fun `http 401 is unauthorized regardless of the code`() {
        val error = playbackException(PlaybackException.ERROR_CODE_IO_BAD_HTTP_STATUS, responseCode = HTTP_UNAUTHORIZED)

        assertEquals(EngineError.Unauthorized, error.toEngineError())
    }

    @Test
    fun `http 403 is unauthorized regardless of the code`() {
        val error = playbackException(PlaybackException.ERROR_CODE_IO_BAD_HTTP_STATUS, responseCode = HTTP_FORBIDDEN)

        assertEquals(EngineError.Unauthorized, error.toEngineError())
    }

    @Test
    fun `http 404 stays unreachable`() {
        val error = playbackException(PlaybackException.ERROR_CODE_IO_BAD_HTTP_STATUS, responseCode = HTTP_NOT_FOUND)

        assertEquals(EngineError.SourceUnreachable, error.toEngineError())
    }

    @Test
    fun `a response code buried deep in the cause chain is still found`() {
        val buried =
            PlaybackException(
                "wrapped",
                IllegalStateException("outer", RuntimeException("inner", invalidResponseCode(HTTP_FORBIDDEN))),
                PlaybackException.ERROR_CODE_IO_UNSPECIFIED,
            )

        assertEquals(EngineError.Unauthorized, buried.toEngineError())
    }

    @Test
    fun `a self-referential cause chain terminates`() {
        val looping = LoopingException()

        val error = PlaybackException("looping", looping, PlaybackException.ERROR_CODE_UNSPECIFIED).toEngineError()

        assertIsUnknown(error)
    }

    /**
     * The whole point of the taxonomy: a network failure must not send the viewer to a second
     * engine that will fail identically, and a format failure must.
     */
    @Test
    fun `only format and decode failures offer the other player`() {
        val offering =
            listOf(
                PlaybackException.ERROR_CODE_PARSING_CONTAINER_UNSUPPORTED,
                PlaybackException.ERROR_CODE_DECODING_FAILED,
            )
        offering.forEach { code ->
            assertTrue(playbackException(code).toEngineError().offersOtherPlayer, "code $code should offer")
        }

        val notOffering =
            listOf(
                PlaybackException.ERROR_CODE_IO_NETWORK_CONNECTION_FAILED,
                PlaybackException.ERROR_CODE_TIMEOUT,
                PlaybackException.ERROR_CODE_IO_INVALID_HTTP_CONTENT_TYPE,
            )
        notOffering.forEach { code ->
            assertFalse(playbackException(code).toEngineError().offersOtherPlayer, "code $code should not offer")
        }
    }

    /**
     * TECH_SPEC §12: an Xtream locator carries the subscriber's username and password in its path,
     * and ExoPlayer puts the failing URL in the exception message. The diagnostic must be built
     * from the code name, never from the message.
     */
    @Test
    fun `unknown detail never echoes the exception message`() {
        val leaky =
            PlaybackException(
                "Unable to connect to http://host/live/subscriber/hunter2/931.ts",
                IOException("Unable to connect to http://host/live/subscriber/hunter2/931.ts"),
                PlaybackException.ERROR_CODE_UNSPECIFIED,
            )

        val unknown = assertIsUnknown(leaky.toEngineError())

        assertFalse(unknown.detail.contains("hunter2"), "detail leaked a credential: '${unknown.detail}'")
        assertFalse(unknown.detail.contains("subscriber"), "detail leaked a credential: '${unknown.detail}'")
        assertFalse(unknown.detail.contains("host"), "detail leaked the origin: '${unknown.detail}'")
    }

    private fun assertIsUnknown(error: EngineError): EngineError.Unknown {
        val unknown = error as? EngineError.Unknown
        return assertNotNull(unknown, "expected Unknown, was $error")
    }

    private fun playbackException(
        errorCode: Int,
        responseCode: Int? = null,
    ): PlaybackException =
        PlaybackException(
            "test",
            responseCode?.let { invalidResponseCode(it) },
            errorCode,
        )

    /**
     * The DataSpec is mocked rather than built: `DataSpec.Builder.setUri` routes through
     * `android.net.Uri.parse`, which has no JVM implementation. The mapping only ever reads
     * [InvalidResponseCodeException.responseCode], so the spec's contents are irrelevant here.
     */
    private fun invalidResponseCode(responseCode: Int): InvalidResponseCodeException =
        InvalidResponseCodeException(
            responseCode,
            "test",
            null,
            emptyMap(),
            mockk<DataSpec>(relaxed = true),
            ByteArray(0),
        )

    /** A throwable whose cause is itself — the shape that turns a naive chain walk into a hang. */
    private class LoopingException : RuntimeException("looping") {
        override val cause: Throwable get() = this
    }

    private companion object {
        const val HTTP_UNAUTHORIZED = 401
        const val HTTP_FORBIDDEN = 403
        const val HTTP_NOT_FOUND = 404
    }
}
