// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.feature.browse

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import dev.spidola.tv.core.corekit.ActionableError
import dev.spidola.tv.core.corekit.BrowseAccess
import dev.spidola.tv.core.corekit.LoadState
import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.toImmutableList
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import uniffi.core_api.ApiException
import uniffi.core_api.BrowseGroup
import uniffi.core_api.MediaKind

/**
 * The type → category level of the browse drill-down for one source: the media kinds present, then
 * the groups for the selected kind. For an M3U source there is a single kind (`LIVE`), so the kind
 * selector never appears (PRD §8.3).
 */
data class SourceBrowseContent(
    val kinds: ImmutableList<MediaKind>,
    val kind: MediaKind,
    val groups: ImmutableList<BrowseGroup>,
)

class SourceBrowseViewModel(
    private val sourceId: Long,
    private val access: BrowseAccess,
) : ViewModel() {
    private val _state = MutableStateFlow<LoadState<SourceBrowseContent>>(LoadState.Loading)
    val state: StateFlow<LoadState<SourceBrowseContent>> = _state.asStateFlow()

    init {
        load()
    }

    fun load() {
        viewModelScope.launch {
            _state.value = LoadState.Loading
            _state.value =
                try {
                    resolve()
                } catch (e: CancellationException) {
                    throw e
                } catch (e: ApiException) {
                    LoadState.Failed(ActionableError.from(e))
                }
        }
    }

    fun selectKind(kind: MediaKind) {
        val current = _state.value
        if (current !is LoadState.Ready || current.value.kind == kind) return
        viewModelScope.launch {
            _state.value =
                try {
                    groupsFor(current.value.kinds, kind)
                } catch (e: CancellationException) {
                    throw e
                } catch (e: ApiException) {
                    LoadState.Failed(ActionableError.from(e))
                }
        }
    }

    private suspend fun resolve(): LoadState<SourceBrowseContent> {
        val kinds = access.kinds(sourceId)
        val first = kinds.firstOrNull() ?: return LoadState.Empty
        return groupsFor(kinds.toImmutableList(), first)
    }

    private suspend fun groupsFor(
        kinds: ImmutableList<MediaKind>,
        kind: MediaKind,
    ): LoadState<SourceBrowseContent> {
        // Groups are bounded (distinct playlist categories) and virtualized in the list; one
        // generous page is loaded and the display lazily renders it.
        val groups = access.groups(sourceId, kind, 0u, GROUP_LIMIT).groups
        return if (groups.isEmpty()) {
            LoadState.Empty
        } else {
            LoadState.Ready(SourceBrowseContent(kinds, kind, groups.toImmutableList()))
        }
    }

    companion object {
        fun factory(
            sourceId: Long,
            access: BrowseAccess,
        ): ViewModelProvider.Factory =
            viewModelFactory {
                initializer { SourceBrowseViewModel(sourceId, access) }
            }

        private const val GROUP_LIMIT = 1000u
    }
}
