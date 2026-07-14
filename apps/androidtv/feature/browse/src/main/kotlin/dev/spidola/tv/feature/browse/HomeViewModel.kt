// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.feature.browse

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import dev.spidola.tv.core.corekit.ActionableError
import dev.spidola.tv.core.corekit.HomeAccess
import dev.spidola.tv.core.corekit.LoadState
import dev.spidola.tv.core.corekit.PlayableChannel
import dev.spidola.tv.core.corekit.common
import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.toImmutableList
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import uniffi.core_api.ApiException
import uniffi.core_api.Source

/** The home screen's content: the sources to browse, plus the favorites and recents rows. */
data class HomeContent(
    val sources: ImmutableList<Source>,
    val favorites: ImmutableList<PlayableChannel>,
    val recents: ImmutableList<PlayableChannel>,
    val recentsEnabled: Boolean,
)

/**
 * Loads the home screen — the source list first, then the favorites and recents rows (PRD §8.3) —
 * and exposes it as immutable state. Recents are empty when the off-switch is set (PRD §6.5), so
 * the screen omits the row. Depends on the narrow [HomeAccess]; unit-tested against a fake.
 */
class HomeViewModel(
    private val access: HomeAccess,
) : ViewModel() {
    private val _state = MutableStateFlow<LoadState<HomeContent>>(LoadState.Loading)
    val state: StateFlow<LoadState<HomeContent>> = _state.asStateFlow()

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
                    throw e // cancellation propagates end-to-end; never swallowed
                } catch (e: ApiException) {
                    LoadState.Failed(ActionableError.from(e))
                }
        }
    }

    /** Toggles the recently-watched off-switch (PRD §6.5) and reloads so the row appears/disappears. */
    fun setRecentsEnabled(enabled: Boolean) = run { access.setRecentsEnabled(enabled) }

    /** Purges the recently-watched list (PRD §6.5) and reloads. */
    fun clearRecents() = run { access.clearRecents() }

    private fun run(action: suspend () -> Unit) {
        viewModelScope.launch {
            try {
                action()
                load()
            } catch (e: CancellationException) {
                throw e
            } catch (e: ApiException) {
                android.util.Log.w("spidola::browse", "recents action failed", e)
            }
        }
    }

    private suspend fun resolve(): LoadState<HomeContent> {
        val sources = access.sources()
        if (sources.none { it.common.enabled }) return LoadState.Empty
        val favorites = access.favoriteChannels(0u, ROW_LIMIT).channels.map { PlayableChannel.of(it) }
        val recentsEnabled = access.recentsEnabled()
        val recents =
            if (recentsEnabled) {
                access.recents(ROW_LIMIT).map { PlayableChannel.of(it) }
            } else {
                emptyList()
            }
        return LoadState.Ready(
            HomeContent(
                sources = sources.toImmutableList(),
                favorites = favorites.toImmutableList(),
                recents = recents.toImmutableList(),
                recentsEnabled = recentsEnabled,
            ),
        )
    }

    companion object {
        fun factory(access: HomeAccess): ViewModelProvider.Factory =
            viewModelFactory {
                initializer { HomeViewModel(access) }
            }

        private const val ROW_LIMIT = 60u
    }
}
