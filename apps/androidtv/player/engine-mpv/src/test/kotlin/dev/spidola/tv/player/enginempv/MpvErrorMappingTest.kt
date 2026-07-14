// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.player.enginempv

import dev.spidola.tv.core.playercontract.EngineError
import dev.spidola.tv.core.playercontract.offersOtherPlayer
import org.junit.jupiter.api.Nested
import org.junit.jupiter.api.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertIs
import kotlin.test.assertNull
import kotlin.test.assertTrue

/**
 * Every branch of the mapping that decides what a viewer is told when a stream fails, and
 * whether they are offered another engine.
 *
 * Runs on the JVM with no libmpv present — which is the point of keeping the mapping pure.
 */
class MpvErrorMappingTest {
    @Nested
    inner class EndFileReasons {
        @Test
        fun `eof is not a failure`() {
            assertNull(
                MpvErrorMapping.endFileError(
                    reason = MpvErrorMapping.EndFileReason.EOF,
                    errorCode = MpvErrorMapping.Code.SUCCESS,
                    diagnostic = null,
                ),
            )
        }

        @Test
        fun `stop is not a failure`() {
            assertNull(
                MpvErrorMapping.endFileError(MpvErrorMapping.EndFileReason.STOP, MpvErrorMapping.Code.SUCCESS, null),
            )
        }

        @Test
        fun `quit is not a failure`() {
            assertNull(
                MpvErrorMapping.endFileError(MpvErrorMapping.EndFileReason.QUIT, MpvErrorMapping.Code.SUCCESS, null),
            )
        }

        @Test
        fun `redirect is not a failure`() {
            assertNull(
                MpvErrorMapping.endFileError(
                    MpvErrorMapping.EndFileReason.REDIRECT,
                    MpvErrorMapping.Code.SUCCESS,
                    null,
                ),
            )
        }

        @Test
        fun `an unknown future reason is not reported as a failure`() {
            // mpv growing a reason must not invent an error the viewer cannot act on.
            assertNull(MpvErrorMapping.endFileError(reason = 99, errorCode = -13, diagnostic = "whatever"))
        }

        @Test
        fun `error reason classifies through the error code`() {
            val error =
                MpvErrorMapping.endFileError(
                    reason = MpvErrorMapping.EndFileReason.ERROR,
                    errorCode = MpvErrorMapping.Code.UNKNOWN_FORMAT,
                    diagnostic = null,
                )
            assertEquals(EngineError.UnsupportedFormat, error)
        }
    }

    @Nested
    inner class ErrorCodes {
        @Test
        fun `unknown format is an unsupported format`() {
            assertEquals(
                EngineError.UnsupportedFormat,
                MpvErrorMapping.engineErrorFor(MpvErrorMapping.Code.UNKNOWN_FORMAT, null),
            )
        }

        @Test
        fun `nothing to play is an unsupported format`() {
            assertEquals(
                EngineError.UnsupportedFormat,
                MpvErrorMapping.engineErrorFor(MpvErrorMapping.Code.NOTHING_TO_PLAY, null),
            )
        }

        @Test
        fun `unsupported is an unsupported format`() {
            assertEquals(
                EngineError.UnsupportedFormat,
                MpvErrorMapping.engineErrorFor(MpvErrorMapping.Code.UNSUPPORTED, null),
            )
        }

        @Test
        fun `video output init failure is a decoder failure`() {
            assertEquals(
                EngineError.DecoderFailed,
                MpvErrorMapping.engineErrorFor(MpvErrorMapping.Code.VO_INIT_FAILED, null),
            )
        }

        @Test
        fun `audio output init failure is a decoder failure`() {
            assertEquals(
                EngineError.DecoderFailed,
                MpvErrorMapping.engineErrorFor(MpvErrorMapping.Code.AO_INIT_FAILED, null),
            )
        }

        @Test
        fun `generic is unknown and carries the code for the log`() {
            val error = MpvErrorMapping.engineErrorFor(MpvErrorMapping.Code.GENERIC, null)
            assertIs<EngineError.Unknown>(error)
            assertTrue(error.detail.contains("-20"))
        }

        @Test
        fun `an unmapped code is unknown rather than a guess`() {
            val error = MpvErrorMapping.engineErrorFor(errorCode = -7, diagnostic = null)
            assertIs<EngineError.Unknown>(error)
            assertTrue(error.detail.contains("-7"))
        }

        @Test
        fun `loading failed with no diagnostic stays unknown so the other player is still offered`() {
            // The reasoning in MpvErrorMapping: -13 is usually a network failure, but guessing
            // SourceUnreachable would suppress "Try other player" for a stream another engine
            // could have played. This test is the guard on that decision.
            val error = MpvErrorMapping.engineErrorFor(MpvErrorMapping.Code.LOADING_FAILED, null)
            assertIs<EngineError.Unknown>(error)
        }
    }

    @Nested
    inner class DiagnosticClassification {
        @Test
        fun `http 401 is unauthorized`() {
            assertEquals(
                EngineError.Unauthorized,
                MpvErrorMapping.engineErrorFor(-13, "[ffmpeg] http: HTTP error 401 Unauthorized"),
            )
        }

        @Test
        fun `http 403 is unauthorized`() {
            assertEquals(
                EngineError.Unauthorized,
                MpvErrorMapping.engineErrorFor(-13, "[ffmpeg] http: HTTP error 403 Forbidden"),
            )
        }

        @Test
        fun `dns failure is unreachable`() {
            assertEquals(
                EngineError.SourceUnreachable,
                MpvErrorMapping.engineErrorFor(-13, "[ffmpeg] tcp: Name or service not known"),
            )
        }

        @Test
        fun `connection refused is unreachable`() {
            assertEquals(
                EngineError.SourceUnreachable,
                MpvErrorMapping.engineErrorFor(-13, "[ffmpeg] tcp: Connection refused"),
            )
        }

        @Test
        fun `network unreachable is unreachable`() {
            assertEquals(
                EngineError.SourceUnreachable,
                MpvErrorMapping.engineErrorFor(-13, "[ffmpeg] tcp: Network is unreachable"),
            )
        }

        @Test
        fun `timeout is a timeout`() {
            assertEquals(
                EngineError.Timeout,
                MpvErrorMapping.engineErrorFor(-13, "[ffmpeg] tcp: Connection timed out"),
            )
        }

        @Test
        fun `invalid data is an unsupported format`() {
            assertEquals(
                EngineError.UnsupportedFormat,
                MpvErrorMapping.engineErrorFor(-13, "[ffmpeg] Invalid data found when processing input"),
            )
        }

        @Test
        fun `missing decoder is an unsupported format`() {
            assertEquals(
                EngineError.UnsupportedFormat,
                MpvErrorMapping.engineErrorFor(-13, "[ffmpeg] No decoder for codec hevc"),
            )
        }

        @Test
        fun `decode error is a decoder failure`() {
            assertEquals(
                EngineError.DecoderFailed,
                MpvErrorMapping.engineErrorFor(-13, "[ffmpeg/video] h264: Error while decoding frame"),
            )
        }

        @Test
        fun `auth outranks a later connection reset`() {
            // A 401 body arrives over a connection that plainly worked; a reset later in the
            // same buffer must not outrank it.
            assertEquals(
                EngineError.Unauthorized,
                MpvErrorMapping.engineErrorFor(-13, "HTTP error 401 Unauthorized | tcp: Connection reset"),
            )
        }

        @Test
        fun `decode outranks format because it is the more specific claim`() {
            assertEquals(
                EngineError.DecoderFailed,
                MpvErrorMapping.engineErrorFor(-13, "Error while decoding frame | unsupported"),
            )
        }

        @Test
        fun `classification is case insensitive`() {
            assertEquals(
                EngineError.Unauthorized,
                MpvErrorMapping.engineErrorFor(-13, "http error 401 unauthorized"),
            )
        }

        @Test
        fun `a blank diagnostic falls through to the code`() {
            val error = MpvErrorMapping.engineErrorFor(MpvErrorMapping.Code.UNKNOWN_FORMAT, "   ")
            assertEquals(EngineError.UnsupportedFormat, error)
        }

        @Test
        fun `an unrecognised diagnostic falls through to the code and is kept for the log`() {
            val error = MpvErrorMapping.engineErrorFor(-20, "something nobody has seen before")
            assertIs<EngineError.Unknown>(error)
            assertTrue(error.detail.contains("something nobody has seen before"))
        }
    }

    @Nested
    inner class FallbackPolicy {
        // The mapping's real product consequence: which failures offer "Try other player".
        // TECH_SPEC §8 says only format/decode failures may, because a network or auth
        // failure would fail identically on any engine.

        @Test
        fun `format and decode failures offer the other player`() {
            val error = MpvErrorMapping.engineErrorFor(-13, "Invalid data found when processing input")
            assertTrue(error.offersOtherPlayer)
            assertTrue(MpvErrorMapping.engineErrorFor(-13, "Error while decoding frame").offersOtherPlayer)
        }

        @Test
        fun `network and auth failures do not offer the other player`() {
            assertFalse(MpvErrorMapping.engineErrorFor(-13, "HTTP error 401 Unauthorized").offersOtherPlayer)
            assertFalse(MpvErrorMapping.engineErrorFor(-13, "Connection refused").offersOtherPlayer)
            assertFalse(MpvErrorMapping.engineErrorFor(-13, "Connection timed out").offersOtherPlayer)
        }
    }
}
