// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.core.designsystem

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.unit.dp
import androidx.tv.material3.Border
import androidx.tv.material3.ClickableSurfaceBorder
import androidx.tv.material3.ClickableSurfaceDefaults
import androidx.tv.material3.ClickableSurfaceScale

/**
 * The Test-Card Amber focus treatment (PRD §8.4: "the focused element is always unmistakable —
 * scale plus Test-Card Amber underline/border"). These ride the native tv-material3 focus
 * behavior rather than fighting it: features hand them to a `Surface`/`Card` so every focusable
 * gets the same amber border and lift, and nothing else in the app uses amber.
 */
object SpidolaFocus {
    /** Corner radius shared by focusable cards and tiles. */
    val cardShape: Shape = RoundedCornerShape(12.dp)

    /** Amber border stroke width when focused. */
    private val borderWidth = 3.dp

    /** Focus lift; kept under the reduce-motion-safe ceiling (all motion < 200 ms, PRD §8.6). */
    private const val FOCUSED_SCALE = 1.05f

    @Composable
    fun border(shape: Shape = cardShape): ClickableSurfaceBorder =
        ClickableSurfaceDefaults.border(
            border = Border.None,
            focusedBorder =
                Border(
                    border = BorderStroke(borderWidth, SpidolaPalette.TestCardAmber),
                    shape = shape,
                ),
        )

    @Composable
    fun scale(): ClickableSurfaceScale = ClickableSurfaceDefaults.scale(focusedScale = FOCUSED_SCALE)
}
