// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

@file:Suppress("MatchingDeclarationName") // Named for the public composable; the data type is a helper.

package dev.spidola.tv.core.designsystem

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.tv.material3.ClickableSurfaceDefaults
import androidx.tv.material3.MaterialTheme
import androidx.tv.material3.Surface
import androidx.tv.material3.Text

/** A trailing accessory on a [SpidolaRow] — a status word or a favorite star. */
sealed interface RowAccessory {
    data object None : RowAccessory

    data class Label(
        val value: String,
    ) : RowAccessory

    data object Star : RowAccessory
}

/**
 * A full-width, D-pad-focusable list row: a title, an optional subtitle, and an optional trailing
 * accessory, wearing the Test-Card Amber focus treatment (PRD §8.4). It rides the native
 * tv-material3 focus behavior via [SpidolaFocus]. Used across the sources, groups, and channel
 * lists. Focus is owned by the caller's list; this renders one clickable surface.
 */
@Composable
fun SpidolaRow(
    title: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    subtitle: String? = null,
    accessory: RowAccessory = RowAccessory.None,
) {
    Surface(
        onClick = onClick,
        modifier = modifier.fillMaxWidth(),
        shape = ClickableSurfaceDefaults.shape(shape = SpidolaFocus.cardShape),
        colors =
            ClickableSurfaceDefaults.colors(
                containerColor = SpidolaPalette.Set,
                contentColor = SpidolaPalette.BroadcastWhite,
            ),
        scale = SpidolaFocus.scale(),
        border = SpidolaFocus.border(),
    ) {
        Row(
            modifier = Modifier.padding(SpidolaSpacing.m),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(SpidolaSpacing.m),
        ) {
            Column(modifier = Modifier.weight(1f)) {
                Text(text = title, style = MaterialTheme.typography.bodyLarge, maxLines = 1)
                if (subtitle != null) {
                    Text(
                        text = subtitle,
                        style = MaterialTheme.typography.labelMedium,
                        color = SpidolaPalette.Static,
                        maxLines = 1,
                    )
                }
            }
            when (accessory) {
                RowAccessory.None -> Unit
                is RowAccessory.Label ->
                    Text(
                        text = accessory.value,
                        style = MaterialTheme.typography.labelMedium,
                        color = SpidolaPalette.Static,
                    )
                RowAccessory.Star ->
                    Text(
                        text = "★",
                        style = MaterialTheme.typography.bodyLarge,
                        color = SpidolaPalette.TestCardAmber,
                    )
            }
        }
    }
}
