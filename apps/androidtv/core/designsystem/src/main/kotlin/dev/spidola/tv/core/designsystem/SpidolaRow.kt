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
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.tv.material3.ClickableSurfaceDefaults
import androidx.tv.material3.MaterialTheme
import androidx.tv.material3.Surface
import androidx.tv.material3.Text

/**
 * A trailing accessory on a [SpidolaRow] — a status word or a favorite star.
 *
 * **Every accessory is silent to a screen reader, and the caller announces what it means.** An
 * accessory is a glance: a word or a mark placed where the eye already is, saying what this row
 * *is* right now. Read aloud it becomes part of the row's name instead — "Keep recently watched,
 * On" — which buries a fact TalkBack has a place of its own for, and then says it twice over for
 * any caller that also put it there properly. So the slot carries no semantics, and a caller that
 * shows one owes its meaning to `stateDescription` (or `selected`, where the row is a choice).
 * Nothing here is lost by that: it is the same fact, moved to where a listener expects it.
 */
sealed interface RowAccessory {
    data object None : RowAccessory

    data class Label(
        val value: String,
    ) : RowAccessory

    /**
     * The mark a row wears to stand out from its neighbours — a favorite, or the choice in force.
     * What it means is especially the caller's to say: it reads as a star to anyone who can see
     * one, and the callers that use it mean three different things by it.
     */
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
            // Cleared once, around the slot rather than inside each arm: the rule belongs to what an
            // accessory *is*, so a fourth kind added below inherits it instead of having to
            // remember it.
            Row(modifier = Modifier.clearAndSetSemantics { }) {
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
}
