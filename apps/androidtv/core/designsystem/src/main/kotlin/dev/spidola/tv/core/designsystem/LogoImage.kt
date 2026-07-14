// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.core.designsystem

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.layout.ContentScale
import coil3.compose.AsyncImage

/**
 * A lazy, disk-cached channel logo (TECH_SPEC §6: "Images via Coil with a capped disk cache").
 * Artwork is the one subsystem allowed network access outside the core, because logo URLs are
 * public by nature and never touch credentials. A neutral tile shows while loading or when there
 * is no logo, so a broken logo never blocks the grid; Coil decodes off the main thread and caches
 * on disk. The settings-driven cache cap wires in when the settings surface lands (Phase 6).
 */
@Composable
fun LogoImage(
    url: String?,
    modifier: Modifier = Modifier,
) {
    Box(
        modifier = modifier.clip(SpidolaFocus.cardShape).background(SpidolaPalette.Set),
    ) {
        if (url != null) {
            AsyncImage(
                model = url,
                contentDescription = null,
                modifier = Modifier.fillMaxSize(),
                contentScale = ContentScale.Fit,
            )
        }
    }
}
