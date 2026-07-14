// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.feature.search

import androidx.compose.foundation.background
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.TextStyle
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.tv.material3.MaterialTheme
import androidx.tv.material3.Text
import dev.spidola.tv.core.corekit.PlayableChannel
import dev.spidola.tv.core.corekit.SearchAccess
import dev.spidola.tv.core.corekit.id
import dev.spidola.tv.core.corekit.label
import dev.spidola.tv.core.corekit.name
import dev.spidola.tv.core.designsystem.RowAccessory
import dev.spidola.tv.core.designsystem.SpidolaPalette
import dev.spidola.tv.core.designsystem.SpidolaRow
import dev.spidola.tv.core.designsystem.SpidolaSpacing
import uniffi.core_api.MediaKind

/**
 * The global search screen: a text field with per-keystroke results, source and media-kind filters,
 * and a focusable result list (PRD §9). Selecting a result opens its detail. The screen owns the
 * query and filter state; the view model owns the debounced search.
 */
@Composable
fun SearchScreen(
    access: SearchAccess,
    onOpenChannel: (PlayableChannel) -> Unit,
    modifier: Modifier = Modifier,
    viewModel: SearchViewModel = viewModel(factory = SearchViewModel.factory(access)),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val sources by viewModel.sources.collectAsStateWithLifecycle()

    var query by rememberSaveable { mutableStateOf("") }
    var sourceFilter by rememberSaveable { mutableStateOf<Long?>(null) }
    var kindFilter by rememberSaveable { mutableStateOf<MediaKind?>(null) }

    fun runSearch() = viewModel.search(query, sourceFilter, kindFilter)

    Column(
        modifier =
            modifier
                .fillMaxSize()
                .background(SpidolaPalette.Studio)
                .padding(horizontal = SpidolaSpacing.safeHorizontal, vertical = SpidolaSpacing.safeVertical),
        verticalArrangement = Arrangement.spacedBy(SpidolaSpacing.m),
    ) {
        Box(modifier = Modifier.fillMaxWidth().background(SpidolaPalette.Set).padding(SpidolaSpacing.m)) {
            if (query.isEmpty()) {
                Text("Search channels", style = MaterialTheme.typography.titleLarge, color = SpidolaPalette.Static)
            }
            BasicTextField(
                value = query,
                onValueChange = {
                    query = it
                    runSearch()
                },
                singleLine = true,
                textStyle = MaterialTheme.typography.titleLarge.merge(TextStyle(color = SpidolaPalette.BroadcastWhite)),
                cursorBrush = SolidColor(SpidolaPalette.TestCardAmber),
                modifier = Modifier.fillMaxWidth().testTag("search-field"),
            )
        }
        Row(
            modifier = Modifier.horizontalScroll(rememberScrollState()),
            horizontalArrangement = Arrangement.spacedBy(SpidolaSpacing.s),
        ) {
            FilterChip("All sources", selected = sourceFilter == null) {
                sourceFilter = null
                runSearch()
            }
            sources.forEach { source ->
                FilterChip(source.name, selected = sourceFilter == source.id) {
                    sourceFilter = source.id
                    runSearch()
                }
            }
        }
        Row(
            modifier = Modifier.horizontalScroll(rememberScrollState()),
            horizontalArrangement = Arrangement.spacedBy(SpidolaSpacing.s),
        ) {
            FilterChip("All types", selected = kindFilter == null) {
                kindFilter = null
                runSearch()
            }
            MediaKind.entries.forEach { kind ->
                FilterChip(kind.label, selected = kindFilter == kind) {
                    kindFilter = kind
                    runSearch()
                }
            }
        }
        Results(state = state, query = query, onOpenChannel = onOpenChannel)
    }
}

@Composable
private fun FilterChip(
    label: String,
    selected: Boolean,
    onClick: () -> Unit,
) {
    SpidolaRow(
        title = label,
        accessory =
            if (selected) {
                RowAccessory.Star
            } else {
                RowAccessory.None
            },
        onClick = onClick,
    )
}

@Composable
private fun Results(
    state: SearchState,
    query: String,
    onOpenChannel: (PlayableChannel) -> Unit,
) {
    when (state) {
        SearchState.Idle -> Hint("Type to search across your channels.")
        SearchState.Loading -> Hint("Searching…")
        SearchState.Empty -> Hint("No channels match “$query”.")
        is SearchState.Failed ->
            ActionableErrorContent(state.error, onRetry = {}, onGoBack = {})
        is SearchState.Results ->
            LazyColumn(
                modifier = Modifier.fillMaxSize(),
                contentPadding = PaddingValues(vertical = SpidolaSpacing.s),
                verticalArrangement = Arrangement.spacedBy(SpidolaSpacing.s),
            ) {
                if (state.results.fuzzy) {
                    item {
                        Text(
                            text = "Showing closest matches",
                            style = MaterialTheme.typography.labelMedium,
                            color = SpidolaPalette.Static,
                        )
                    }
                }
                items(state.results.channels, key = { it.identity }) { channel ->
                    SpidolaRow(
                        title = channel.name,
                        subtitle = channel.groupTitle,
                        onClick = { onOpenChannel(PlayableChannel.of(channel)) },
                        modifier = Modifier.testTag("search-result-${channel.name}"),
                    )
                }
            }
    }
}

@Composable
private fun Hint(message: String) {
    Box(modifier = Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
        Text(text = message, style = MaterialTheme.typography.bodyLarge, color = SpidolaPalette.Static)
    }
}
