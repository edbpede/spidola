// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

@file:Suppress("MatchingDeclarationName") // Named for the ribbon; the bars are the palette it draws.

package dev.spidola.tv.core.designsystem

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.unit.dp
import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.persistentListOf

/**
 * The SMPTE colour-bar spectrum, tuned into Spidola's tonal family.
 *
 * The bars are the classic seven, in broadcast order (white → yellow → cyan → green → magenta → red
 * → blue). Three of them are palette values the app already owns: the yellow bar **is**
 * [SpidolaPalette.TestCardAmber], which is where the accent came from in the first place (PRD §8.2),
 * and red/green are the stream-health pair. The remaining four are derived to sit at the same
 * saturation and luminance, so the ribbon reads as one muted band rather than a strip of toy
 * primaries on a near-black canvas.
 *
 * This is the only decorative element in the app (PRD §8.5). It earns its place by explaining the
 * accent: the viewer sees where Test-Card Amber comes from every time the strip appears.
 */
object SmpteBars {
    private val Cyan = Color(0xFF5FA8A8)
    private val Magenta = Color(0xFFA2618F)
    private val Blue = Color(0xFF4E6A9E)

    /** The bars in broadcast left-to-right order. */
    val ordered: ImmutableList<Color> =
        persistentListOf(
            SpidolaPalette.BroadcastWhite,
            SpidolaPalette.TestCardAmber,
            Cyan,
            SpidolaPalette.StreamGreen,
            Magenta,
            SpidolaPalette.StreamRed,
            Blue,
        )
}

/**
 * The three-point SMPTE ribbon that underlines the channel strip (PRD §8.5).
 *
 * Deliberately not focusable and cleared from the accessibility tree: it is decoration, and a screen
 * reader stopping on a colour bar would be noise between the channel name and the zap controls.
 */
@Composable
fun SmpteRibbon(modifier: Modifier = Modifier) {
    Row(
        modifier =
            modifier
                .fillMaxWidth()
                .height(RibbonHeight)
                .clearAndSetSemantics { },
    ) {
        SmpteBars.ordered.forEach { bar ->
            Box(modifier = Modifier.weight(1f).fillMaxHeight().background(bar))
        }
    }
}

/** Three points at 10 feet is a hairline that reads as a broadcast artefact rather than a border. */
private val RibbonHeight = 3.dp
