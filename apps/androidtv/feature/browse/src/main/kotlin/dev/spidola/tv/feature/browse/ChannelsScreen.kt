// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.feature.browse

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
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
import dev.spidola.tv.core.corekit.PlayableChannel
import dev.spidola.tv.core.designsystem.RowAccessory
import dev.spidola.tv.core.designsystem.SpidolaPalette
import dev.spidola.tv.core.designsystem.SpidolaRow
import dev.spidola.tv.core.designsystem.SpidolaSpacing
import kotlinx.collections.immutable.ImmutableList
import uniffi.core_api.MediaKind

/**
 * The channel level of the drill-down: the visible channels in a group, D-pad-focusable and
 * virtualized (paged on scroll), each marking favorites with a star. Selecting a channel opens its
 * detail, where play / favorite / hide live (the D-pad-first equivalent of a context menu).
 */
@Suppress("ViewModelForwarding") // The screen owns the VM and hands it to its single list child.
@Composable
fun ChannelsScreen(
    sourceId: Long,
    kind: MediaKind,
    group: String?,
    access: BrowseAccess,
    navigator: BrowseNavigator,
    modifier: Modifier = Modifier,
    viewModel: ChannelsViewModel =
        viewModel(factory = ChannelsViewModel.factory(sourceId, kind, group, access)),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    Box(modifier = modifier.fillMaxSize().background(SpidolaPalette.Studio)) {
        when (val current = state) {
            LoadState.Loading -> CenteredMessage("Loading channels…")
            LoadState.Empty -> CenteredMessage("No channels here.")
            is LoadState.Failed ->
                ActionableErrorContent(current.error, onRetry = viewModel::load, onGoBack = {})
            is LoadState.Ready -> ChannelList(current.value, navigator, viewModel)
        }
    }
}

@Composable
private fun ChannelList(
    rows: ImmutableList<ChannelRow>,
    navigator: BrowseNavigator,
    viewModel: ChannelsViewModel,
) {
    val firstRow = remember { FocusRequester() }
    LaunchedEffect(rows.firstOrNull()?.key) {
        if (rows.isNotEmpty()) firstRow.requestFocus()
    }

    // A row whose offset the list can no longer resolve has left the ring, so opening it would zap
    // from a position it no longer occupies.
    fun open(row: ChannelRow) {
        val offset = viewModel.offsetOf(row) ?: return
        navigator.openChannel(PlayableChannel.of(row.channel), viewModel.zapContext, offset)
    }
    LazyColumn(
        modifier = Modifier.fillMaxSize().focusRestorer(firstRow),
        contentPadding =
            PaddingValues(
                horizontal = SpidolaSpacing.safeHorizontal,
                vertical = SpidolaSpacing.safeVertical,
            ),
        verticalArrangement = Arrangement.spacedBy(SpidolaSpacing.s),
    ) {
        items(rows, key = { it.key }) { row ->
            LaunchedEffect(row.key) { viewModel.loadMoreIfNeeded(row) }
            SpidolaRow(
                title = row.channel.name,
                subtitle = row.channel.groupTitle,
                accessory = if (row.isFavorite) RowAccessory.Star else RowAccessory.None,
                onClick = { open(row) },
                modifier =
                    Modifier
                        .testTag("channel-${row.channel.name}")
                        .then(if (row.key == rows.first().key) Modifier.focusRequester(firstRow) else Modifier),
            )
        }
    }
}
