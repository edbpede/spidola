// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.core.designsystem

import android.provider.Settings
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.tv.material3.Border
import androidx.tv.material3.ClickableSurfaceBorder
import androidx.tv.material3.ClickableSurfaceDefaults
import androidx.tv.material3.ClickableSurfaceScale

/**
 * Whether the viewer has asked the system to remove animations.
 *
 * Android has no `LocalReduceMotion`, so this reads the animator duration scale the accessibility
 * settings write: `0f` is the platform's own way of saying "remove animations", and it is what
 * "Remove animations" sets. Public because reduce-motion is a P0 bar for *every* slice (PRD
 * §6.10) — the channel strip's slide and content crossfades have to honour it too, and each
 * inventing its own read is how one of them ends up not honouring it.
 *
 * Read once per composition rather than observed: the setting is a system-wide preference a user
 * sets and leaves, and the alternative is a `ContentObserver` per focusable surface on a TV.
 */
@Composable
fun rememberReduceMotion(): Boolean {
    val resolver = LocalContext.current.contentResolver
    return remember(resolver) {
        Settings.Global.getFloat(resolver, Settings.Global.ANIMATOR_DURATION_SCALE, 1f) == 0f
    }
}

/**
 * The Test-Card Amber focus treatment (PRD §8.4: "the focused element is always unmistakable —
 * scale plus Test-Card Amber underline/border"). These ride the native tv-material3 focus
 * behavior rather than fighting it: features hand them to a `Surface`/`Card` so every focusable
 * gets the same amber border and lift, and nothing else in the app uses amber.
 */
object SpidolaFocus {
    /** Corner radius shared by focusable cards and tiles. */
    val cardShape: Shape = RoundedCornerShape(12.dp)

    /**
     * Amber border stroke width when focused. Public so the rare focusable that a `Surface` cannot
     * express — a read-only line in the diagnostics log viewer, which is focusable so the D-pad can
     * scroll it but is not clickable — wears the same focus ring as everything else, rather than
     * declaring a second 3.dp of its own.
     */
    val borderWidth = 3.dp

    /** Focus lift, applied unless the viewer has asked for animations to be removed. */
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

    /**
     * The focus lift, suppressed under reduce-motion (PRD §6.10, §8.6).
     *
     * Only the *movement* goes. The amber border in [border] stays, because reduce-motion asks for
     * less animation, not less legibility — a focus treatment a viewer cannot see is a worse
     * accessibility failure than the one being fixed, and D-pad navigation is unusable without it
     * (PRD §8.4: the focused element is always unmistakable).
     *
     * Honoured here rather than at each call site because this *is* the app's focus motion: every
     * focusable surface takes its scale from this one function, so reading the setting once makes
     * the rule true everywhere at once instead of true wherever someone remembered. An earlier
     * comment here claimed the 1.05 lift was "kept under the reduce-motion-safe ceiling (< 200 ms)"
     * — that conflated duration with suppression. Short motion is still motion; reduce-motion means
     * none.
     */
    @Composable
    fun scale(): ClickableSurfaceScale =
        ClickableSurfaceDefaults.scale(
            focusedScale = if (rememberReduceMotion()) 1f else FOCUSED_SCALE,
        )
}
