// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

@file:Suppress("MatchingDeclarationName") // Named for the public composable; the data type is a helper.

package dev.spidola.tv.core.designsystem

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.focusRestorer
import androidx.compose.ui.unit.dp
import androidx.tv.material3.ClickableSurfaceDefaults
import androidx.tv.material3.MaterialTheme
import androidx.tv.material3.Surface
import androidx.tv.material3.Text
import kotlinx.collections.immutable.ImmutableList

/**
 * One card in a [PosterRail]. [id] is a caller-composed stable key (e.g. source+identity), so it
 * doubles as the lazy-list key that `focusRestorer` needs and the focus key across a data refresh.
 */
data class PosterItem(
    val id: String,
    val title: String,
    val subtitle: String?,
    val logo: String?,
)

/**
 * A titled, horizontally-scrolling rail of poster cards — the home screen's favorites and recents
 * rows (PRD §8.3). D-pad focus moves card to card on the foundation [LazyRow] with pivot scrolling;
 * `focusRestorer` with stable keys restores focus on back-navigation. An empty rail renders nothing
 * so the caller can omit the section entirely.
 */
@Composable
fun PosterRail(
    title: String,
    items: ImmutableList<PosterItem>,
    onSelect: (PosterItem) -> Unit,
    modifier: Modifier = Modifier,
) {
    if (items.isEmpty()) return
    Column(
        modifier = modifier,
        verticalArrangement = Arrangement.spacedBy(SpidolaSpacing.s),
    ) {
        Text(
            text = title,
            style = MaterialTheme.typography.titleLarge,
            color = SpidolaPalette.BroadcastWhite,
            modifier = Modifier.padding(horizontal = SpidolaSpacing.safeHorizontal),
        )
        LazyRow(
            modifier = Modifier.fillMaxWidth().focusRestorer(),
            contentPadding = PaddingValues(horizontal = SpidolaSpacing.safeHorizontal),
            horizontalArrangement = Arrangement.spacedBy(SpidolaSpacing.m),
        ) {
            items(items = items, key = { it.id }) { item ->
                PosterCard(item = item, onClick = { onSelect(item) })
            }
        }
    }
}

@Composable
private fun PosterCard(
    item: PosterItem,
    onClick: () -> Unit,
) {
    Surface(
        onClick = onClick,
        modifier = Modifier.width(CARD_WIDTH.dp),
        shape = ClickableSurfaceDefaults.shape(shape = SpidolaFocus.cardShape),
        colors =
            ClickableSurfaceDefaults.colors(
                containerColor = SpidolaPalette.Set,
                contentColor = SpidolaPalette.BroadcastWhite,
            ),
        scale = SpidolaFocus.scale(),
        border = SpidolaFocus.border(),
    ) {
        Column {
            LogoImage(
                url = item.logo,
                modifier = Modifier.fillMaxWidth().aspectRatio(CARD_ASPECT),
            )
            Text(
                text = item.title,
                style = MaterialTheme.typography.labelMedium,
                color = SpidolaPalette.BroadcastWhite,
                maxLines = 1,
                modifier = Modifier.padding(SpidolaSpacing.s),
            )
        }
    }
}

private const val CARD_WIDTH = 240
private const val CARD_ASPECT = 16f / 9f
