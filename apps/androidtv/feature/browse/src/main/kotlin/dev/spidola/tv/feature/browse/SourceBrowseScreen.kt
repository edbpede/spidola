// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.feature.browse

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
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
import dev.spidola.tv.core.corekit.BrowseAccess
import dev.spidola.tv.core.corekit.LoadState
import dev.spidola.tv.core.corekit.label
import dev.spidola.tv.core.designsystem.RowAccessory
import dev.spidola.tv.core.designsystem.SpidolaPalette
import dev.spidola.tv.core.designsystem.SpidolaRow
import dev.spidola.tv.core.designsystem.SpidolaSpacing
import uniffi.core_api.MediaKind

/**
 * The categories screen for one source: an optional media-kind selector (shown only when a source
 * carries more than one kind — Xtream, Phase 6) followed by the virtualized list of groups. A group
 * leads to its channel list.
 */
@Composable
fun SourceBrowseScreen(
    sourceId: Long,
    access: BrowseAccess,
    navigator: BrowseNavigator,
    modifier: Modifier = Modifier,
    viewModel: SourceBrowseViewModel =
        viewModel(factory = SourceBrowseViewModel.factory(sourceId, access)),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    Box(modifier = modifier.fillMaxSize().background(SpidolaPalette.Studio)) {
        when (val current = state) {
            LoadState.Loading -> CenteredMessage("Loading categories…")
            LoadState.Empty ->
                CenteredMessage("This source has no channels yet. Refresh it from the sources screen.")
            is LoadState.Failed ->
                ActionableErrorContent(current.error, onRetry = viewModel::load, onGoBack = {})
            is LoadState.Ready -> Groups(current.value, sourceId, navigator, viewModel::selectKind)
        }
    }
}

@Composable
private fun Groups(
    content: SourceBrowseContent,
    sourceId: Long,
    navigator: BrowseNavigator,
    onSelectKind: (MediaKind) -> Unit,
) {
    val firstGroup = remember { FocusRequester() }
    LaunchedEffect(content.groups.firstOrNull()?.title) {
        if (content.groups.isNotEmpty()) firstGroup.requestFocus()
    }
    LazyColumn(
        modifier = Modifier.fillMaxSize().focusRestorer(firstGroup),
        contentPadding =
            PaddingValues(
                horizontal = SpidolaSpacing.safeHorizontal,
                vertical = SpidolaSpacing.safeVertical,
            ),
        verticalArrangement = Arrangement.spacedBy(SpidolaSpacing.s),
    ) {
        if (content.kinds.size > 1) {
            item { KindSelector(content, onSelectKind) }
        }
        itemsIndexed(content.groups, key = { _, group -> group.title ?: "" }) { index, group ->
            val title = group.title ?: "Ungrouped"
            SpidolaRow(
                title = title,
                accessory = RowAccessory.Label(group.channelCount.toString()),
                onClick = { navigator.openChannels(sourceId, content.kind, group.title, title) },
                modifier =
                    Modifier
                        .testTag("group-$title")
                        .then(if (index == 0) Modifier.focusRequester(firstGroup) else Modifier),
            )
        }
    }
}

@Composable
private fun KindSelector(
    content: SourceBrowseContent,
    onSelectKind: (MediaKind) -> Unit,
) {
    Row(horizontalArrangement = Arrangement.spacedBy(SpidolaSpacing.m)) {
        content.kinds.forEach { kind ->
            SpidolaRow(
                title = kind.label,
                accessory = if (kind == content.kind) RowAccessory.Star else RowAccessory.None,
                onClick = { onSelectKind(kind) },
                modifier = Modifier.padding(bottom = SpidolaSpacing.s),
            )
        }
    }
}
