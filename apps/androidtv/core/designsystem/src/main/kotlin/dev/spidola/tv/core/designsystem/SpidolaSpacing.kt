// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.core.designsystem

import androidx.compose.ui.unit.dp

/**
 * The spacing scale and TV-safe margins (PRD §8.4: "All content respects TV-safe margins").
 * The safe insets keep content off the ~5% overscan region every panel can clip.
 */
object SpidolaSpacing {
    val xs = 4.dp
    val s = 8.dp
    val m = 16.dp
    val l = 24.dp
    val xl = 48.dp

    /** Horizontal TV-safe inset (PRD §8.4). */
    val safeHorizontal = 48.dp

    /** Vertical TV-safe inset (PRD §8.4). */
    val safeVertical = 27.dp
}
