// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.core.designsystem

import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.sp
import androidx.tv.material3.Typography

// Numerals are tabular everywhere times or channel numbers appear (PRD §8.3).
private const val TABULAR = "tnum"

// Body/UI text uses the platform system face (Roboto on Android TV) for 10-foot legibility.
// The display face is a characterful grotesque (Archivo, SIL OFL) — bundled as a font resource
// in a later slice; the scale below encodes its weights, sizes, and tracking now so the display
// role is ready the moment the asset lands. Until then it falls back to the system sans face.
private val DisplayFamily = FontFamily.Default

/**
 * The short, strict type scale from PRD §8.3 — display, title, body, caption — with the
 * minimum body size at the platform's 10-foot floor. No text below caption is ever focusable.
 */
object SpidolaType {
    val display =
        TextStyle(
            fontFamily = DisplayFamily,
            fontWeight = FontWeight.W800,
            fontSize = 45.sp,
            lineHeight = 52.sp,
            letterSpacing = 0.5.sp,
            fontFeatureSettings = TABULAR,
        )

    val title =
        TextStyle(
            fontWeight = FontWeight.W700,
            fontSize = 26.sp,
            lineHeight = 32.sp,
            fontFeatureSettings = TABULAR,
        )

    val body =
        TextStyle(
            fontWeight = FontWeight.W400,
            fontSize = 20.sp,
            lineHeight = 26.sp,
            fontFeatureSettings = TABULAR,
        )

    val caption =
        TextStyle(
            fontWeight = FontWeight.W500,
            fontSize = 16.sp,
            lineHeight = 20.sp,
            fontFeatureSettings = TABULAR,
        )
}

/** Maps the Spidola scale onto the TV Material 3 type roles the components read. */
val SpidolaTypography: Typography =
    Typography(
        displayLarge = SpidolaType.display,
        titleLarge = SpidolaType.title,
        bodyLarge = SpidolaType.body,
        labelMedium = SpidolaType.caption,
    )
