// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.core.designsystem

import androidx.compose.runtime.Composable
import androidx.tv.material3.MaterialTheme
import androidx.tv.material3.darkColorScheme

// Dark-first is a considered choice (PRD §8.1): the app lives on living-room panels in dim
// rooms over full-motion video. There is exactly one scheme — there is no light variant.
private val SpidolaColorScheme =
    darkColorScheme(
        primary = SpidolaPalette.TestCardAmber,
        onPrimary = SpidolaPalette.Studio,
        background = SpidolaPalette.Studio,
        onBackground = SpidolaPalette.BroadcastWhite,
        surface = SpidolaPalette.Studio,
        onSurface = SpidolaPalette.BroadcastWhite,
        surfaceVariant = SpidolaPalette.Set,
        onSurfaceVariant = SpidolaPalette.Static,
        error = SpidolaPalette.StreamRed,
    )

/** Wraps the app in the Spidola TV Material 3 theme: the palette and type scale from PRD §8. */
@Composable
fun SpidolaTheme(content: @Composable () -> Unit) {
    MaterialTheme(
        colorScheme = SpidolaColorScheme,
        typography = SpidolaTypography,
        content = content,
    )
}
