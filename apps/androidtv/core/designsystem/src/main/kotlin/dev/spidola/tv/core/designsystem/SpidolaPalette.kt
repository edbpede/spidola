// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.core.designsystem

import androidx.compose.ui.graphics.Color

/**
 * The five named palette values from PRD §8.2, plus the two muted semantic colors reserved
 * for stream-health / error contexts. These are the only colors in the app: Test-Card Amber
 * marks exactly focus, the live indicator, and primary actions and appears nowhere else.
 */
object SpidolaPalette {
    /** Canvas — a near-black with a cool cast; the base surface. */
    val Studio = Color(0xFF12151A)

    /** Raised surface for cards, rails, and overlays. */
    val Set = Color(0xFF1C2129)

    /** Primary text — a warm paper-white that reads softly at 10 feet. */
    val BroadcastWhite = Color(0xFFF1EFE9)

    /** Secondary text and inactive metadata. */
    val Static = Color(0xFF8B94A3)

    /** The single accent (SMPTE yellow bar): focus, the live indicator, primary actions only. */
    val TestCardAmber = Color(0xFFE3A44A)

    /**
     * Stream-health / error only, muted into the same tonal family as the rest.
     *
     * Light enough to carry prose: this is the one semantic color that reaches text (a
     * validation message, and Material's `error` role), and at the Set surface's 4.5:1 floor
     * that sets its lightness, not taste. PRD §8.2 pins no hex here — it asks only for a muted
     * red in the same tonal family — so the hue and saturation are the design and the lightness
     * is the constraint.
     */
    val StreamRed = Color(0xFFC96E69)

    /** Stream-health only, muted into the same tonal family. */
    val StreamGreen = Color(0xFF6FA36A)
}
