// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.feature.browse

import android.util.Log
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import dev.spidola.tv.core.corekit.CatalogAccess
import dev.spidola.tv.core.corekit.id
import kotlinx.collections.immutable.toImmutableList
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import uniffi.core_api.ApiException
import uniffi.core_api.Channel

/**
 * Loads the first source's first page of channels for the walking skeleton (M0) and exposes it
 * as immutable UI state. It depends on the narrow [CatalogAccess] interface, not the concrete
 * core, so it is unit-tested against a fake. The full source/type/category drill-down is Phase 4.
 */
class BrowseViewModel(
    private val catalog: CatalogAccess,
) : ViewModel() {
    private val _state = MutableStateFlow<BrowseUiState>(BrowseUiState.Loading)
    val state: StateFlow<BrowseUiState> = _state.asStateFlow()

    init {
        load()
    }

    fun load() {
        viewModelScope.launch {
            _state.value = BrowseUiState.Loading
            _state.value =
                try {
                    resolveState()
                } catch (e: CancellationException) {
                    throw e // cancellation propagates end-to-end; never swallowed
                } catch (e: ApiException) {
                    // Phase 4 maps each ApiError variant to a plain-language class + actions
                    // (PRD §6.3); the diagnostic chain stays in the log stream, not on screen.
                    Log.w(LOG_TAG, "channel load failed", e)
                    BrowseUiState.Error("Couldn't load channels — try again.")
                }
        }
    }

    private suspend fun resolveState(): BrowseUiState {
        val sourceId = catalog.sources().firstOrNull()?.id ?: return BrowseUiState.Empty
        val channels = catalog.page(sourceId, PAGE_OFFSET, PAGE_LIMIT).channels
        return if (channels.isEmpty()) {
            BrowseUiState.Empty
        } else {
            BrowseUiState.Ready(channels.map(Channel::toItem).toImmutableList())
        }
    }

    companion object {
        /** Builds the view-model from the injected catalog (manual DI; see AppContainer). */
        fun factory(catalog: CatalogAccess): ViewModelProvider.Factory =
            viewModelFactory {
                initializer { BrowseViewModel(catalog) }
            }

        private const val LOG_TAG = "spidola::browse"
        private const val PAGE_OFFSET = 0u
        private const val PAGE_LIMIT = 200u
    }
}

private fun Channel.toItem(): ChannelItem = ChannelItem(key = identity, name = name, group = groupTitle)
