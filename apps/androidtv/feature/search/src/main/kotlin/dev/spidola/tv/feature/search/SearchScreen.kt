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
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.TextStyle
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.tv.material3.MaterialTheme
import androidx.tv.material3.Text
import dev.spidola.tv.core.corekit.PlayableChannel
import dev.spidola.tv.core.corekit.SearchAccess
import dev.spidola.tv.core.corekit.ZapContext
import dev.spidola.tv.core.corekit.id
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
    onOpenChannel: (channel: PlayableChannel, context: ZapContext, offset: UInt) -> Unit,
    modifier: Modifier = Modifier,
    initialQuery: String = "",
    viewModel: SearchViewModel = viewModel(factory = SearchViewModel.factory(access)),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val sources by viewModel.sources.collectAsStateWithLifecycle()

    var query by rememberSaveable(initialQuery) { mutableStateOf(initialQuery) }
    var sourceFilter by rememberSaveable { mutableStateOf<Long?>(null) }
    var kindFilter by rememberSaveable { mutableStateOf<MediaKind?>(null) }

    fun runSearch() = viewModel.search(query, sourceFilter, kindFilter)

    LaunchedEffect(initialQuery) {
        if (initialQuery.isNotBlank()) runSearch()
    }

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
                Text(
                    text = stringResource(R.string.search_placeholder),
                    style = MaterialTheme.typography.titleLarge,
                    color = SpidolaPalette.Static,
                )
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
            FilterChip(stringResource(R.string.search_all_sources), selected = sourceFilter == null) {
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
            FilterChip(stringResource(R.string.search_all_types), selected = kindFilter == null) {
                kindFilter = null
                runSearch()
            }
            MediaKind.entries.forEach { kind ->
                FilterChip(kind.label(), selected = kindFilter == kind) {
                    kindFilter = kind
                    runSearch()
                }
            }
        }
        Results(state = state, query = query, onOpenChannel = onOpenChannel)
    }
}

/**
 * One filter in a row of them, starred when it is the one in force. The star is silent, so the chip
 * announces what it means instead: which filter is applied is the whole content of these two rows,
 * and a viewer who cannot see the mark would otherwise hear the same sentence for every source they
 * own (PRD §6.10). Only the applied chip says it — the rest are visibly the alternatives.
 */
@Composable
private fun FilterChip(
    label: String,
    selected: Boolean,
    onClick: () -> Unit,
) {
    val selectedState = stringResource(R.string.search_filter_selected)
    SpidolaRow(
        title = label,
        accessory =
            if (selected) {
                RowAccessory.Star
            } else {
                RowAccessory.None
            },
        onClick = onClick,
        modifier = if (selected) Modifier.semantics { stateDescription = selectedState } else Modifier,
    )
}

@Composable
private fun Results(
    state: SearchState,
    query: String,
    onOpenChannel: (channel: PlayableChannel, context: ZapContext, offset: UInt) -> Unit,
) {
    when (state) {
        SearchState.Idle -> Hint(stringResource(R.string.search_idle))
        SearchState.Loading -> Hint(stringResource(R.string.search_loading))
        SearchState.Empty -> Hint(stringResource(R.string.search_empty, query))
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
                            text = stringResource(R.string.search_fuzzy),
                            style = MaterialTheme.typography.labelMedium,
                            color = SpidolaPalette.Static,
                        )
                    }
                }
                // The set is fetched from offset 0 in score order, so a row's index in it is its
                // offset in the search ring.
                itemsIndexed(state.results.channels, key = { _, channel -> channel.identity }) { offset, channel ->
                    SpidolaRow(
                        title = channel.name,
                        subtitle = channel.groupTitle,
                        onClick = {
                            onOpenChannel(PlayableChannel.of(channel), state.results.context, offset.toUInt())
                        },
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
