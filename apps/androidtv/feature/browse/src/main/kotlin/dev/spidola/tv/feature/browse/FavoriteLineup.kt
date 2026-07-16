// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

@file:Suppress("MatchingDeclarationName")

package dev.spidola.tv.feature.browse

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.focusRestorer
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import androidx.tv.material3.MaterialTheme
import androidx.tv.material3.Text
import dev.spidola.tv.core.corekit.ActionableError
import dev.spidola.tv.core.corekit.HomeAccess
import dev.spidola.tv.core.corekit.LoadState
import dev.spidola.tv.core.corekit.PlayableChannel
import dev.spidola.tv.core.designsystem.SpidolaPalette
import dev.spidola.tv.core.designsystem.SpidolaRow
import dev.spidola.tv.core.designsystem.SpidolaSpacing
import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.toImmutableList
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import uniffi.core_api.ApiException

class FavoriteLineupViewModel(
    private val access: HomeAccess,
) : ViewModel() {
    private val _state = MutableStateFlow<LoadState<ImmutableList<PlayableChannel>>>(LoadState.Loading)
    val state: StateFlow<LoadState<ImmutableList<PlayableChannel>>> = _state.asStateFlow()

    init {
        load()
    }

    fun moveUp(index: Int) {
        val channels = (_state.value as? LoadState.Ready)?.value ?: return
        if (index <= 0) return
        move { access.moveFavoriteBefore(channels[index], channels[index - 1]) }
    }

    fun moveDown(index: Int) {
        val channels = (_state.value as? LoadState.Ready)?.value ?: return
        if (index >= channels.lastIndex) return
        move { access.moveFavoriteAfter(channels[index], channels[index + 1]) }
    }

    fun load() {
        viewModelScope.launch {
            _state.value =
                try {
                    val channels = mutableListOf<PlayableChannel>()
                    var total = ULong.MAX_VALUE
                    while (channels.size.toULong() < total) {
                        val page = access.favoriteChannels(channels.size.toUInt(), PAGE_LIMIT)
                        total = page.total
                        channels += page.channels.map(PlayableChannel::of)
                        if (page.channels.isEmpty()) break
                    }
                    val lineup = channels.toImmutableList()
                    if (lineup.isEmpty()) LoadState.Empty else LoadState.Ready(lineup)
                } catch (e: CancellationException) {
                    throw e
                } catch (e: ApiException) {
                    LoadState.Failed(ActionableError.from(e))
                }
        }
    }

    private fun move(action: suspend () -> Unit) {
        viewModelScope.launch {
            try {
                action()
                load()
            } catch (e: CancellationException) {
                throw e
            } catch (e: ApiException) {
                _state.value = LoadState.Failed(ActionableError.from(e))
            }
        }
    }

    companion object {
        private const val PAGE_LIMIT = 200u

        fun factory(access: HomeAccess): ViewModelProvider.Factory {
            return viewModelFactory { initializer { FavoriteLineupViewModel(access) } }
        }
    }
}

@Composable
fun FavoriteLineupScreen(
    access: HomeAccess,
    onGoBack: () -> Unit,
    modifier: Modifier = Modifier,
    viewModel: FavoriteLineupViewModel = viewModel(factory = FavoriteLineupViewModel.factory(access)),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    Column(modifier.fillMaxSize().background(SpidolaPalette.Studio)) {
        Text(
            text = stringResource(R.string.browse_favorites_order_title),
            style = MaterialTheme.typography.displayLarge,
            color = SpidolaPalette.BroadcastWhite,
            modifier = Modifier.padding(SpidolaSpacing.safeHorizontal, SpidolaSpacing.safeVertical),
        )
        when (val current = state) {
            LoadState.Loading -> CenteredMessage(stringResource(R.string.browse_home_loading))
            LoadState.Empty -> CenteredMessage(stringResource(R.string.browse_favorites_order_empty))
            is LoadState.Failed ->
                ActionableErrorContent(current.error, onRetry = viewModel::load, onGoBack = onGoBack)
            is LoadState.Ready -> {
                val firstControl = remember { FocusRequester() }
                LazyColumn(
                    modifier = Modifier.fillMaxSize().focusRestorer(firstControl),
                    contentPadding = PaddingValues(horizontal = SpidolaSpacing.safeHorizontal),
                    verticalArrangement = Arrangement.spacedBy(SpidolaSpacing.s),
                ) {
                    itemsIndexed(current.value, key = { _, channel -> channel.key }) { index, channel ->
                        Row(horizontalArrangement = Arrangement.spacedBy(SpidolaSpacing.m)) {
                            Text(
                                text = (index + 1).toString().padStart(2, '0'),
                                style = MaterialTheme.typography.titleLarge,
                                color = SpidolaPalette.TestCardAmber,
                                modifier = Modifier.width(56.dp).padding(top = SpidolaSpacing.m),
                            )
                            Text(
                                text = channel.name,
                                style = MaterialTheme.typography.bodyLarge,
                                color = SpidolaPalette.BroadcastWhite,
                                modifier = Modifier.weight(1f).padding(top = SpidolaSpacing.m),
                            )
                            SpidolaRow(
                                title = stringResource(R.string.browse_favorites_move_up),
                                onClick = { viewModel.moveUp(index) },
                                modifier =
                                    Modifier
                                        .width(190.dp)
                                        .then(if (index == 0) Modifier.focusRequester(firstControl) else Modifier)
                                        .testTag("favorite-up-${channel.key}"),
                            )
                            SpidolaRow(
                                title = stringResource(R.string.browse_favorites_move_down),
                                onClick = { viewModel.moveDown(index) },
                                modifier = Modifier.width(190.dp).testTag("favorite-down-${channel.key}"),
                            )
                        }
                    }
                }
            }
        }
    }
}
