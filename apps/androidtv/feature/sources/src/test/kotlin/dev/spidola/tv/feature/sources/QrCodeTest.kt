// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.feature.sources

import com.google.zxing.BinaryBitmap
import com.google.zxing.RGBLuminanceSource
import com.google.zxing.common.HybridBinarizer
import com.google.zxing.qrcode.QRCodeReader
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertNotNull
import org.junit.jupiter.api.Assertions.assertNull
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test

class QrCodeTest {
    @Test
    fun `a pairing url round-trips through the code a phone would scan`() {
        val url = "http://192.168.1.40:53219"
        val matrix = qrMatrixOf(url)
        assertNotNull(matrix)

        // Decoding what we encoded is the only assertion that means anything here: a matrix of the
        // right shape that no camera can read would pass every structural check and fail the one
        // job this has.
        assertEquals(url, decode(matrix!!))
    }

    @Test
    fun `a long url still round-trips`() {
        // Nothing bounds the input's length, so the encoder has to pick a version that fits rather
        // than truncate. Deliberately longer than any real pairing URL — `core-pair` builds
        // `http://{host}:{port}` and nothing else, and the token is typed into the form, never
        // carried in the address — so this is a headroom check, not a realistic case.
        val url = "http://192.168.100.200:65535/pair/" + "segment/".repeat(8)
        val matrix = qrMatrixOf(url)
        assertNotNull(matrix)
        assertEquals(url, decode(matrix!!))
    }

    @Test
    fun `an unencodable input is an absent code, not a crash`() {
        // The screen shows the address and token as text regardless, so a code that cannot be built
        // costs a convenience rather than the feature — and there is nothing a viewer could do
        // about it, so it must not throw or raise an error at them.
        assertNull(qrMatrixOf(""))
    }

    @Test
    fun `the matrix is square and carries dark modules`() {
        val matrix = qrMatrixOf("http://10.0.0.7:53219")
        assertNotNull(matrix)
        assertTrue(matrix!!.size > 0)
        val dark = (0 until matrix.size).sumOf { y -> (0 until matrix.size).count { x -> matrix[x, y] } }
        assertTrue(dark > 0, "a QR with no dark modules is a blank plate")
    }

    /** Renders the matrix to pixels and reads it back with zxing's decoder — the phone's job. */
    private fun decode(matrix: QrMatrix): String {
        val scale = 4
        val side = matrix.size * scale
        val pixels =
            IntArray(side * side) { i ->
                val x = (i % side) / scale
                val y = (i / side) / scale
                if (matrix[x, y]) 0xFF000000.toInt() else 0xFFFFFFFF.toInt()
            }
        val source = RGBLuminanceSource(side, side, pixels)
        return QRCodeReader().decode(BinaryBitmap(HybridBinarizer(source))).text
    }
}
