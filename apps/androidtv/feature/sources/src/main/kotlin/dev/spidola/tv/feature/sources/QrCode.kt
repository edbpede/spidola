// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

@file:Suppress("MatchingDeclarationName") // Named for the public composable; the matrix is its input.

package dev.spidola.tv.feature.sources

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import com.google.zxing.BarcodeFormat
import com.google.zxing.EncodeHintType
import com.google.zxing.qrcode.QRCodeWriter
import com.google.zxing.qrcode.decoder.ErrorCorrectionLevel

/** The modules a QR of this URL needs, and whether each is dark. A plain grid, so the drawing is
 * just rectangles and the encoder stays out of the composable. */
internal class QrMatrix(
    val size: Int,
    private val dark: BooleanArray,
) {
    operator fun get(
        x: Int,
        y: Int,
    ): Boolean = dark[y * size + x]
}

/**
 * Encodes [content] as a QR code, or returns `null` if it cannot be encoded.
 *
 * `null` rather than a throw: the pairing screen shows the address and code as text regardless, so
 * a QR that cannot be built costs a convenience, not the feature. There is nothing for a viewer to
 * do about it and nothing to say, so an actionable error would be a lie (PRD §6.3) — the code just
 * isn't drawn.
 */
internal fun qrMatrixOf(content: String): QrMatrix? =
    runCatching {
        // A margin of 1 module: zxing's default quiet zone is 4, which at TV distance wastes a
        // third of the code's width on nothing. The Canvas draws on a white plate that supplies the
        // rest of the quiet zone optically.
        val hints = mapOf(EncodeHintType.MARGIN to 1, EncodeHintType.ERROR_CORRECTION to ErrorCorrectionLevel.M)
        val matrix = QRCodeWriter().encode(content, BarcodeFormat.QR_CODE, 0, 0, hints)
        val size = matrix.width
        BooleanArray(size * matrix.height) { i -> matrix[i % size, i / size] }.let { QrMatrix(size, it) }
    }.getOrNull()

/**
 * Draws a QR of the pairing URL, so a phone can point a camera instead of a person typing an IP
 * address off a television.
 *
 * Drawn as rectangles on a Canvas rather than rendered to a bitmap: a QR *is* a grid of squares, the
 * matrix is tiny, and this keeps the code crisp at any size with no bitmap to scale, allocate, or
 * recycle.
 *
 * [contentDescription] describes the code for a screen reader, which cannot see it. The address and
 * token are also on screen as text, so nothing here is the only way to get the information.
 */
@Composable
internal fun QrCode(
    matrix: QrMatrix,
    contentDescription: String,
    modifier: Modifier = Modifier,
) {
    // The code is dark-on-light regardless of the app's dark canvas: a QR scanner expects the dark
    // modules to be the data, and inverting it stops many phone cameras from reading it at all.
    val plate = Color.White
    val module = Color.Black
    val cells = remember(matrix) { matrix.size }

    Canvas(
        modifier = modifier.aspectRatio(1f).semantics { this.contentDescription = contentDescription },
    ) {
        drawRect(color = plate, size = size)
        val cell = size.minDimension / cells
        for (y in 0 until cells) {
            for (x in 0 until cells) {
                if (!matrix[x, y]) continue
                drawRect(
                    color = module,
                    topLeft = Offset(x * cell, y * cell),
                    // A hair over one cell, so neighbouring modules meet rather than leaving seams
                    // that a camera reads as gaps.
                    size = Size(cell + 1f, cell + 1f),
                )
            }
        }
    }
}
