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
import dev.spidola.tv.core.corekit.ZapContext
import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.toImmutableList
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import uniffi.core_api.ApiException
import uniffi.core_api.Channel
import uniffi.core_api.MediaKind

/** One channel row plus its favorite flag, so the list marks favorites without a per-row query. */
data class ChannelRow(
    val channel: Channel,
    val isFavorite: Boolean,
) {
    /** The stable identity — the lazy-list/focus key that survives a refresh. */
    val key: Long get() = channel.identity
}

/**
 * The channel level of the browse drill-down: the visible channels in one group, paged by contract
 * and appended as the user scrolls (virtualized), with the per-channel favorite and hide actions
 * the context row drives. Hidden channels are excluded by the core, so hiding one drops it.
 */
class ChannelsViewModel(
    private val sourceId: Long,
    private val kind: MediaKind,
    private val group: String?,
    private val access: BrowseAccess,
) : ViewModel() {
    private val _state = MutableStateFlow<LoadState<ImmutableList<ChannelRow>>>(LoadState.Loading)
    val state: StateFlow<LoadState<ImmutableList<ChannelRow>>> = _state.asStateFlow()

    private var favorites: Set<Long> = emptySet()
    private val rows = mutableListOf<ChannelRow>()
    private var total: ULong = 0uL
    private var paging = false

    init {
        load()
    }

    /** The ring a channel opened from this list zaps through: this list's own query (PRD §8.4). */
    val zapContext: ZapContext get() = ZapContext.Group(sourceId, kind, group)

    /**
     * [row]'s absolute position in the ring, or `null` once it has left the list.
     *
     * Pages are appended in order from offset 0 and a hidden row leaves both this list and the
     * core's, so a row's index here is its offset in the core's — the value the zap ring is keyed
     * on. Callers resolve this on selection, never per render.
     */
    fun offsetOf(row: ChannelRow): UInt? = rows.indexOfFirst { it.key == row.key }.takeIf { it >= 0 }?.toUInt()

    fun load() {
        viewModelScope.launch {
            _state.value = LoadState.Loading
            favorites = emptySet()
            rows.clear()
            total = 0uL
            _state.value =
                try {
                    favorites = access.favoriteIdentities(sourceId).toSet()
                    val page = access.channelsInGroup(sourceId, kind, group, 0u, PAGE_LIMIT)
                    total = page.total
                    append(page.channels)
                    readyOrEmpty()
                } catch (e: CancellationException) {
                    throw e
                } catch (e: ApiException) {
                    LoadState.Failed(ActionableError.from(e))
                }
        }
    }

    /** Loads the next page when [row] nears the loaded tail. Paging failures keep what is shown. */
    fun loadMoreIfNeeded(row: ChannelRow) {
        if (_state.value !is LoadState.Ready || paging || rows.size.toULong() >= total) return
        val index = rows.indexOfFirst { it.key == row.key }
        if (index < 0 || index < rows.size - PREFETCH_MARGIN) return
        paging = true
        viewModelScope.launch {
            try {
                val page = access.channelsInGroup(sourceId, kind, group, rows.size.toUInt(), PAGE_LIMIT)
                append(page.channels)
                _state.value = LoadState.Ready(rows.toImmutableList())
            } catch (e: CancellationException) {
                throw e
            } catch (_: ApiException) {
                // Keep the rows already loaded; the next scroll retries.
            } finally {
                paging = false
            }
        }
    }

    fun toggleFavorite(row: ChannelRow) {
        val makeFavorite = row.key !in favorites
        setFavorite(row.key, makeFavorite) // optimistic
        viewModelScope.launch {
            try {
                access.setFavorite(sourceId, row.channel.identity, makeFavorite)
            } catch (e: CancellationException) {
                throw e
            } catch (_: ApiException) {
                setFavorite(row.key, !makeFavorite) // revert on failure
            }
        }
    }

    fun hide(row: ChannelRow) {
        viewModelScope.launch {
            try {
                access.setHidden(sourceId, row.channel.identity, true)
                rows.removeAll { it.key == row.key }
                _state.value = readyOrEmpty()
            } catch (e: CancellationException) {
                throw e
            } catch (_: ApiException) {
                // Leave the row in place; the user can try again.
            }
        }
    }

    private fun append(channels: List<Channel>) {
        channels.forEach { channel ->
            rows.add(ChannelRow(channel, channel.identity in favorites))
        }
    }

    private fun setFavorite(
        identity: Long,
        isFavorite: Boolean,
    ) {
        favorites = if (isFavorite) favorites + identity else favorites - identity
        val index = rows.indexOfFirst { it.key == identity }
        if (index >= 0) rows[index] = rows[index].copy(isFavorite = isFavorite)
        if (_state.value is LoadState.Ready) _state.value = LoadState.Ready(rows.toImmutableList())
    }

    private fun readyOrEmpty(): LoadState<ImmutableList<ChannelRow>> =
        if (rows.isEmpty()) LoadState.Empty else LoadState.Ready(rows.toImmutableList())

    companion object {
        fun factory(
            sourceId: Long,
            kind: MediaKind,
            group: String?,
            access: BrowseAccess,
        ): ViewModelProvider.Factory =
            viewModelFactory {
                initializer { ChannelsViewModel(sourceId, kind, group, access) }
            }

        private const val PAGE_LIMIT = 200u
        private const val PREFETCH_MARGIN = 20
    }
}
