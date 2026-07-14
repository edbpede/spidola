// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.feature.browse

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.focusRestorer
import androidx.compose.ui.platform.testTag
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.tv.material3.MaterialTheme
import androidx.tv.material3.Text
import dev.spidola.tv.core.corekit.HomeAccess
import dev.spidola.tv.core.corekit.LoadState
import dev.spidola.tv.core.corekit.common
import dev.spidola.tv.core.corekit.id
import dev.spidola.tv.core.corekit.kindLabel
import dev.spidola.tv.core.corekit.name
import dev.spidola.tv.core.designsystem.PosterRail
import dev.spidola.tv.core.designsystem.RowAccessory
import dev.spidola.tv.core.designsystem.SpidolaPalette
import dev.spidola.tv.core.designsystem.SpidolaRow
import dev.spidola.tv.core.designsystem.SpidolaSpacing
import kotlinx.collections.immutable.toImmutableList

/**
 * The home screen (PRD §8.3): the favorites row first, then recently watched, then the sources to
 * browse into, with search and source-management reachable from here. The composition root hands it
 * a [HomeAccess] and a [BrowseNavigator]; it holds no durable state of its own.
 */
@Composable
fun HomeScreen(
    access: HomeAccess,
    navigator: BrowseNavigator,
    modifier: Modifier = Modifier,
    viewModel: HomeViewModel = viewModel(factory = HomeViewModel.factory(access)),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    Box(modifier = modifier.fillMaxSize().background(SpidolaPalette.Studio)) {
        when (val current = state) {
            LoadState.Loading -> CenteredMessage("Loading…")
            LoadState.Empty -> EmptyHome(onAdd = navigator.manageSources)
            is LoadState.Failed ->
                ActionableErrorContent(
                    error = current.error,
                    onRetry = viewModel::load,
                    onGoBack = viewModel::load,
                )
            is LoadState.Ready ->
                HomeReady(
                    content = current.value,
                    navigator = navigator,
                    onToggleRecents = viewModel::setRecentsEnabled,
                    onClearRecents = viewModel::clearRecents,
                )
        }
    }
}

@Composable
private fun EmptyHome(onAdd: () -> Unit) {
    Box(modifier = Modifier.fillMaxSize().padding(SpidolaSpacing.xl)) {
        SpidolaRow(
            title = "Add a source to start watching",
            onClick = onAdd,
            modifier = Modifier.testTag("home-add-source"),
        )
    }
}

@Composable
private fun HomeReady(
    content: HomeContent,
    navigator: BrowseNavigator,
    onToggleRecents: (Boolean) -> Unit,
    onClearRecents: () -> Unit,
) {
    val firstSource = remember { FocusRequester() }
    val enabledSources = content.sources.filter { it.common.enabled }
    LaunchedEffect(enabledSources.firstOrNull()?.id) {
        if (enabledSources.isNotEmpty()) firstSource.requestFocus()
    }
    LazyColumn(
        modifier = Modifier.fillMaxSize().focusRestorer(firstSource),
        contentPadding = PaddingValues(vertical = SpidolaSpacing.safeVertical),
        verticalArrangement = Arrangement.spacedBy(SpidolaSpacing.l),
    ) {
        if (content.favorites.isNotEmpty()) {
            item {
                PosterRail(
                    title = "Favorites",
                    items = content.favorites.map { it.toPoster() }.toImmutableList(),
                    onSelect = { item ->
                        content.favorites.firstOrNull { it.key == item.id }?.let(navigator.openChannel)
                    },
                )
            }
        }
        if (content.recents.isNotEmpty()) {
            item {
                PosterRail(
                    title = "Recently watched",
                    items = content.recents.map { it.toPoster() }.toImmutableList(),
                    onSelect = { item ->
                        content.recents.firstOrNull { it.key == item.id }?.let(navigator.openChannel)
                    },
                )
            }
        }
        item {
            Text(
                text = "Sources",
                style = MaterialTheme.typography.titleLarge,
                color = SpidolaPalette.BroadcastWhite,
                modifier = Modifier.padding(horizontal = SpidolaSpacing.safeHorizontal),
            )
        }
        itemsIndexed(enabledSources, key = { _, source -> "source-${source.id}" }) { index, source ->
            SpidolaRow(
                title = source.name,
                subtitle = source.kindLabel,
                onClick = { navigator.openSource(source.id, source.name) },
                modifier =
                    Modifier
                        .padding(horizontal = SpidolaSpacing.safeHorizontal)
                        .testTag("source-${source.name}")
                        .then(if (index == 0) Modifier.focusRequester(firstSource) else Modifier),
            )
        }
        item {
            SpidolaRow(
                title = "Search channels",
                onClick = navigator.openSearch,
                modifier =
                    Modifier.padding(horizontal = SpidolaSpacing.safeHorizontal).testTag("home-search"),
            )
        }
        item {
            SpidolaRow(
                title = "Add or manage sources",
                onClick = navigator.manageSources,
                modifier =
                    Modifier.padding(horizontal = SpidolaSpacing.safeHorizontal).testTag("home-manage"),
            )
        }
        item {
            Text(
                text = "Recently watched",
                style = MaterialTheme.typography.titleLarge,
                color = SpidolaPalette.BroadcastWhite,
                modifier = Modifier.padding(horizontal = SpidolaSpacing.safeHorizontal),
            )
        }
        item {
            SpidolaRow(
                title = "Keep recently watched",
                accessory = RowAccessory.Label(if (content.recentsEnabled) "On" else "Off"),
                onClick = { onToggleRecents(!content.recentsEnabled) },
                modifier =
                    Modifier.padding(horizontal = SpidolaSpacing.safeHorizontal).testTag("home-recents-toggle"),
            )
        }
        if (content.recents.isNotEmpty()) {
            item {
                SpidolaRow(
                    title = "Clear recently watched",
                    onClick = onClearRecents,
                    modifier =
                        Modifier.padding(horizontal = SpidolaSpacing.safeHorizontal).testTag("home-recents-clear"),
                )
            }
        }
    }
}
