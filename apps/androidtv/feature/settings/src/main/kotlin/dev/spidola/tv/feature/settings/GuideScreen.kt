// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.feature.settings

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.TextStyle
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
import dev.spidola.tv.core.corekit.EpgAccess
import dev.spidola.tv.core.corekit.EpgRefreshEvent
import dev.spidola.tv.core.corekit.EpgWindowSettings
import dev.spidola.tv.core.corekit.LoadState
import dev.spidola.tv.core.corekit.id
import dev.spidola.tv.core.corekit.name
import dev.spidola.tv.core.designsystem.RowAccessory
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
import uniffi.core_api.EpgRefreshStage
import uniffi.core_api.Source

data class GuideSource(
    val source: Source,
    val hasFeed: Boolean,
)

data class GuideContent(
    val sources: ImmutableList<GuideSource>,
    val selectedId: Long? = null,
    val status: GuideStatus? = null,
    val window: EpgWindowSettings,
)

sealed interface GuideStatus {
    data class Refreshing(
        val stage: EpgRefreshStage,
        val seen: ULong,
    ) : GuideStatus

    data class Complete(
        val inserted: ULong,
    ) : GuideStatus
}

class GuideViewModel(
    private val access: EpgAccess,
) : ViewModel() {
    private val _state = MutableStateFlow<LoadState<GuideContent>>(LoadState.Loading)
    val state: StateFlow<LoadState<GuideContent>> = _state.asStateFlow()

    init {
        load()
    }

    fun select(sourceId: Long) {
        val content = content() ?: return
        _state.value = LoadState.Ready(content.copy(selectedId = sourceId, status = null))
    }

    fun setFeed(
        sourceId: Long,
        url: String,
    ) = mutate { access.setXmltvFeed(sourceId, url.trim()) }

    fun clearFeed(sourceId: Long) = mutate { access.clearXmltvFeed(sourceId) }

    fun setWindow(
        aheadHours: UInt,
        behindHours: UInt,
    ) = mutate { access.setEpgWindow(aheadHours, behindHours) }

    fun refresh(sourceId: Long) {
        viewModelScope.launch {
            access.refreshEpg(sourceId, System.currentTimeMillis() / UNIX_MILLIS_PER_SECOND).collect { event ->
                when (event) {
                    is EpgRefreshEvent.Progress ->
                        updateStatus(GuideStatus.Refreshing(event.progress.stage, event.progress.programmesSeen))
                    is EpgRefreshEvent.Complete -> {
                        updateStatus(GuideStatus.Complete(event.outcome.inserted))
                        load()
                    }
                    is EpgRefreshEvent.Failed -> _state.value = LoadState.Failed(ActionableError.from(event.error))
                }
            }
        }
    }

    fun load() {
        viewModelScope.launch {
            val selected = content()?.selectedId
            _state.value =
                try {
                    val sources =
                        access.guideSources().map { source -> GuideSource(source, access.hasEpgFeed(source.id)) }
                    if (sources.isEmpty()) {
                        LoadState.Empty
                    } else {
                        LoadState.Ready(
                            GuideContent(
                                sources = sources.toImmutableList(),
                                selectedId = selected ?: sources.first().source.id,
                                window = access.epgWindowSettings(),
                            ),
                        )
                    }
                } catch (e: CancellationException) {
                    throw e
                } catch (e: ApiException) {
                    LoadState.Failed(ActionableError.from(e))
                }
        }
    }

    private fun mutate(action: suspend () -> Unit) {
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

    private fun content(): GuideContent? = (_state.value as? LoadState.Ready)?.value

    private fun updateStatus(status: GuideStatus) {
        val content = content() ?: return
        _state.value = LoadState.Ready(content.copy(status = status))
    }

    companion object {
        private const val UNIX_MILLIS_PER_SECOND = 1_000L

        fun factory(access: EpgAccess): ViewModelProvider.Factory {
            return viewModelFactory { initializer { GuideViewModel(access) } }
        }
    }
}

@Composable
fun GuideScreen(
    access: EpgAccess,
    onGoBack: () -> Unit,
    modifier: Modifier = Modifier,
    viewModel: GuideViewModel = viewModel(factory = GuideViewModel.factory(access)),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val actions =
        GuideActions(
            select = viewModel::select,
            setFeed = viewModel::setFeed,
            clearFeed = viewModel::clearFeed,
            refresh = viewModel::refresh,
            setWindow = viewModel::setWindow,
        )
    when (val current = state) {
        LoadState.Loading -> Centered(stringResource(R.string.guide_loading))
        LoadState.Empty -> Centered(stringResource(R.string.guide_empty))
        is LoadState.Failed -> ActionableErrorContent(current.error, viewModel::load, onGoBack)
        is LoadState.Ready -> GuideReady(current.value, actions, onGoBack, modifier)
    }
}

private data class GuideActions(
    val select: (Long) -> Unit,
    val setFeed: (Long, String) -> Unit,
    val clearFeed: (Long) -> Unit,
    val refresh: (Long) -> Unit,
    val setWindow: (UInt, UInt) -> Unit,
)

@Composable
private fun GuideReady(
    content: GuideContent,
    actions: GuideActions,
    onGoBack: () -> Unit,
    modifier: Modifier = Modifier,
) {
    var url by remember(content.selectedId) { mutableStateOf("") }
    var aheadHours by remember(content.window) { mutableStateOf(content.window.aheadHours) }
    var behindHours by remember(content.window) { mutableStateOf(content.window.behindHours) }
    val selected = content.sources.firstOrNull { it.source.id == content.selectedId }
    Column(
        modifier
            .fillMaxSize()
            .background(SpidolaPalette.Studio)
            .verticalScroll(rememberScrollState())
            .padding(SpidolaSpacing.safeHorizontal, SpidolaSpacing.safeVertical),
        verticalArrangement = Arrangement.spacedBy(SpidolaSpacing.m),
    ) {
        Text(
            stringResource(R.string.guide_title),
            style = MaterialTheme.typography.displayLarge,
            color = SpidolaPalette.BroadcastWhite,
        )
        content.sources.forEach { item ->
            SpidolaRow(
                title = item.source.name,
                accessory =
                    RowAccessory.Label(
                        stringResource(if (item.hasFeed) R.string.guide_feed_ready else R.string.guide_feed_missing),
                    ),
                onClick = { actions.select(item.source.id) },
            )
        }
        if (selected != null) {
            Text(
                stringResource(R.string.guide_address),
                style = MaterialTheme.typography.labelMedium,
                color = SpidolaPalette.Static,
            )
            BasicTextField(
                value = url,
                onValueChange = { url = it },
                textStyle = MaterialTheme.typography.bodyLarge.merge(TextStyle(color = SpidolaPalette.BroadcastWhite)),
                cursorBrush = SolidColor(SpidolaPalette.TestCardAmber),
                modifier =
                    Modifier
                        .fillMaxWidth()
                        .background(SpidolaPalette.Set)
                        .padding(SpidolaSpacing.m)
                        .testTag("guide-address"),
            )
            SpidolaRow(stringResource(R.string.guide_save), { actions.setFeed(selected.source.id, url) })
            if (selected.hasFeed) {
                SpidolaRow(stringResource(R.string.guide_refresh), { actions.refresh(selected.source.id) })
                SpidolaRow(stringResource(R.string.guide_clear), { actions.clearFeed(selected.source.id) })
            }
        }
        SpidolaRow(
            title = stringResource(R.string.guide_window_ahead),
            accessory = RowAccessory.Label(stringResource(R.string.guide_hours, aheadHours.toInt())),
            onClick = { aheadHours = nextValue(aheadHours, listOf(6u, 12u, 24u, 48u)) },
        )
        SpidolaRow(
            title = stringResource(R.string.guide_window_behind),
            accessory = RowAccessory.Label(stringResource(R.string.guide_hours, behindHours.toInt())),
            onClick = { behindHours = nextValue(behindHours, listOf(0u, 2u, 6u, 12u)) },
        )
        SpidolaRow(
            title = stringResource(R.string.guide_window_save),
            onClick = { actions.setWindow(aheadHours, behindHours) },
        )
        content.status?.let { status ->
            Text(
                text = status.label(),
                style = MaterialTheme.typography.bodyLarge,
                color = SpidolaPalette.TestCardAmber,
            )
        }
        SpidolaRow(stringResource(R.string.guide_cancel), onGoBack)
    }
}

@Composable
private fun GuideStatus.label(): String =
    when (this) {
        is GuideStatus.Complete -> stringResource(R.string.guide_refresh_done, inserted.toLong())
        is GuideStatus.Refreshing ->
            when (stage) {
                EpgRefreshStage.CONNECTING -> stringResource(R.string.guide_refresh_connecting)
                EpgRefreshStage.DOWNLOADING -> stringResource(R.string.guide_refresh_downloading, seen.toLong())
                EpgRefreshStage.FINALIZING -> stringResource(R.string.guide_refresh_finalizing)
            }
    }

private fun nextValue(
    current: UInt,
    values: List<UInt>,
): UInt {
    val index = values.indexOf(current).coerceAtLeast(0)
    return values[(index + 1) % values.size]
}
