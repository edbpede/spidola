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
import dev.spidola.tv.core.corekit.EpgAccess
import dev.spidola.tv.core.corekit.PlayableChannel
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import uniffi.core_api.ApiException
import uniffi.core_api.NowNext

/** The channel detail screen's observable state: the favorite/hidden flags and a transient notice. */
data class ChannelDetailUiState(
    val isFavorite: Boolean = false,
    val isHidden: Boolean = false,
    val notice: ActionableError? = null,
    val schedule: NowNext? = null,
)

/**
 * Backs the channel detail screen: the favorite/hidden flags behind the toggles. Play is not here —
 * it is a navigation intent the shell owns, and the recent is recorded by the playback slice once
 * the stream actually starts. Toggle failures surface as a short notice with the diagnostic detail
 * kept in the log stream (PRD §6.3, §8.6), never swallowed.
 */
class ChannelDetailViewModel(
    val channel: PlayableChannel,
    private val access: BrowseAccess,
    private val epgAccess: EpgAccess,
    private val nowUnix: () -> Long = { System.currentTimeMillis() / UNIX_MILLIS_PER_SECOND },
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
                        schedule = epgAccess.nowNext(channel.sourceId, channel.identity, nowUnix()),
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

    private fun present(error: ApiException) {
        Log.w(LOG_TAG, "detail action failed", error)
        _state.value = _state.value.copy(notice = ActionableError.from(error))
    }

    companion object {
        private const val UNIX_MILLIS_PER_SECOND = 1_000L

        fun factory(
            channel: PlayableChannel,
            access: BrowseAccess,
            epgAccess: EpgAccess,
        ): ViewModelProvider.Factory =
            viewModelFactory {
                initializer { ChannelDetailViewModel(channel, access, epgAccess) }
            }

        private const val LOG_TAG = "spidola::browse"
    }
}
