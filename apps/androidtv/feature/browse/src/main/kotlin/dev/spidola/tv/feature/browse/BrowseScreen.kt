// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.feature.browse

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.focusRestorer
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextAlign
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.tv.material3.ClickableSurfaceDefaults
import androidx.tv.material3.MaterialTheme
import androidx.tv.material3.Surface
import androidx.tv.material3.Text
import dev.spidola.tv.core.corekit.CatalogAccess
import dev.spidola.tv.core.designsystem.SpidolaFocus
import dev.spidola.tv.core.designsystem.SpidolaPalette
import dev.spidola.tv.core.designsystem.SpidolaSpacing
import kotlinx.collections.immutable.ImmutableList

/**
 * The browse vertical slice for the walking skeleton (M0): a D-pad-navigable list of the fixture
 * catalog's channels. Playback on select, and the source → type → category drill-down, land in
 * later phases; this screen proves focus traversal and the core → shell rendering path.
 */
@Composable
fun BrowseScreen(
    catalog: CatalogAccess,
    modifier: Modifier = Modifier,
    viewModel: BrowseViewModel = viewModel(factory = BrowseViewModel.factory(catalog)),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    BrowseContent(state = state, modifier = modifier)
}

@Composable
internal fun BrowseContent(
    state: BrowseUiState,
    modifier: Modifier = Modifier,
) {
    Box(
        modifier = modifier.fillMaxSize().background(SpidolaPalette.Studio),
        contentAlignment = Alignment.TopStart,
    ) {
        when (state) {
            BrowseUiState.Loading -> CenteredMessage("Loading channels…")
            BrowseUiState.Empty -> CenteredMessage("No sources yet — add one to start watching.")
            is BrowseUiState.Error -> CenteredMessage(state.message)
            is BrowseUiState.Ready -> ChannelList(state.channels)
        }
    }
}

@Composable
private fun ChannelList(channels: ImmutableList<ChannelItem>) {
    val firstItem = remember { FocusRequester() }
    LaunchedEffect(Unit) { firstItem.requestFocus() }
    LazyColumn(
        modifier = Modifier.fillMaxSize().focusRestorer(firstItem),
        contentPadding =
            PaddingValues(
                horizontal = SpidolaSpacing.safeHorizontal,
                vertical = SpidolaSpacing.safeVertical,
            ),
        verticalArrangement = Arrangement.spacedBy(SpidolaSpacing.s),
    ) {
        itemsIndexed(channels, key = { _, item -> item.key }) { index, item ->
            ChannelCard(
                item = item,
                modifier =
                    Modifier
                        .testTag("channel-$index")
                        .then(if (index == 0) Modifier.focusRequester(firstItem) else Modifier),
            )
        }
    }
}

@Composable
private fun ChannelCard(
    item: ChannelItem,
    modifier: Modifier = Modifier,
) {
    Surface(
        // Selecting a channel starts playback in Phase 5.
        onClick = {},
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
        Column(modifier = Modifier.padding(SpidolaSpacing.m)) {
            Text(text = item.name, style = MaterialTheme.typography.bodyLarge)
            val group = item.group
            if (group != null) {
                Text(
                    text = group,
                    style = MaterialTheme.typography.labelMedium,
                    color = SpidolaPalette.Static,
                )
            }
        }
    }
}

@Composable
private fun CenteredMessage(message: String) {
    Box(
        modifier = Modifier.fillMaxSize().padding(SpidolaSpacing.xl),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            text = message,
            style = MaterialTheme.typography.titleLarge,
            color = SpidolaPalette.BroadcastWhite,
            textAlign = TextAlign.Center,
        )
    }
}
