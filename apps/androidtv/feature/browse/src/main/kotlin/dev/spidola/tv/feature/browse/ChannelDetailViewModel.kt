// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.feature.browse

import android.util.Log
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import dev.spidola.tv.core.corekit.ActionableError
import dev.spidola.tv.core.corekit.BrowseAccess
import dev.spidola.tv.core.corekit.PlayableChannel
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import uniffi.core_api.ApiException

/** The channel detail screen's observable state: the favorite/hidden flags and a transient notice. */
data class ChannelDetailUiState(
    val isFavorite: Boolean = false,
    val isHidden: Boolean = false,
    val notice: String? = null,
)

/**
 * Backs the channel detail screen: the favorite/hidden flags for the toggles, and the play action
 * that records a recent (Phase 5 wires the actual engine). Toggle failures surface as a short
 * notice with the diagnostic detail kept in the log stream (PRD §6.3, §8.6), never swallowed.
 */
class ChannelDetailViewModel(
    val channel: PlayableChannel,
    private val access: BrowseAccess,
) : ViewModel() {
    private val _state = MutableStateFlow(ChannelDetailUiState())
    val state: StateFlow<ChannelDetailUiState> = _state.asStateFlow()

    init {
        load()
    }

    private fun load() {
        viewModelScope.launch {
            try {
                _state.value =
                    _state.value.copy(
                        isFavorite = access.isFavorite(channel.sourceId, channel.identity),
                        isHidden = access.isHidden(channel.sourceId, channel.identity),
                    )
            } catch (e: CancellationException) {
                throw e
            } catch (e: ApiException) {
                Log.w(LOG_TAG, "detail load failed", e)
            }
        }
    }

    fun toggleFavorite() {
        val makeFavorite = !_state.value.isFavorite
        viewModelScope.launch {
            try {
                access.setFavorite(channel.sourceId, channel.identity, makeFavorite)
                _state.value = _state.value.copy(isFavorite = makeFavorite)
            } catch (e: CancellationException) {
                throw e
            } catch (e: ApiException) {
                present(e)
            }
        }
    }

    fun toggleHidden() {
        val makeHidden = !_state.value.isHidden
        viewModelScope.launch {
            try {
                access.setHidden(channel.sourceId, channel.identity, makeHidden)
                _state.value = _state.value.copy(isHidden = makeHidden)
            } catch (e: CancellationException) {
                throw e
            } catch (e: ApiException) {
                present(e)
            }
        }
    }

    /** Records the channel to recently-watched. Playback itself lands with the engine contract in
     * Phase 5; recording here means the recents row is exercised end-to-end now. */
    fun play() {
        viewModelScope.launch {
            try {
                access.recordRecent(channel)
                _state.value =
                    _state.value.copy(
                        notice = "Saved to Recently watched. Full playback arrives in a later update.",
                    )
            } catch (e: CancellationException) {
                throw e
            } catch (e: ApiException) {
                present(e)
            }
        }
    }

    private fun present(error: ApiException) {
        Log.w(LOG_TAG, "detail action failed", error)
        _state.value = _state.value.copy(notice = ActionableError.from(error).message)
    }

    companion object {
        fun factory(
            channel: PlayableChannel,
            access: BrowseAccess,
        ): ViewModelProvider.Factory =
            viewModelFactory {
                initializer { ChannelDetailViewModel(channel, access) }
            }

        private const val LOG_TAG = "spidola::browse"
    }
}
