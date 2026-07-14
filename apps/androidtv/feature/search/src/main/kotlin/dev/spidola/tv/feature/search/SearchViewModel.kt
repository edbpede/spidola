// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.feature.search

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import dev.spidola.tv.core.corekit.ActionableError
import dev.spidola.tv.core.corekit.SearchAccess
import dev.spidola.tv.core.corekit.ZapContext
import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.persistentListOf
import kotlinx.collections.immutable.toImmutableList
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Job
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import uniffi.core_api.ApiException
import uniffi.core_api.Channel
import uniffi.core_api.MediaKind
import uniffi.core_api.Source

/** Search results plus whether the trigram fuzzy fallback produced them (so the UI can say so). */
data class SearchResults(
    val channels: ImmutableList<Channel>,
    val fuzzy: Boolean,
    /**
     * The ring these results zap through — the query and filters that actually produced them
     * (PRD §8.4). Held with the results rather than read back from the live fields, which the next
     * keystroke has already changed: zapping the ring of a query the viewer never ran would move
     * them to a channel they never saw.
     */
    val context: ZapContext,
)

/** The search screen's phase. `Idle` is the empty-query resting state; the rest mirror a load. */
sealed interface SearchState {
    data object Idle : SearchState

    data object Loading : SearchState

    data object Empty : SearchState

    data class Results(
        val results: SearchResults,
    ) : SearchState

    data class Failed(
        val error: ActionableError,
    ) : SearchState
}

/**
 * Drives global search with per-keystroke results against the core's sub-50 ms budget (PRD §9),
 * plus the source and media-kind filters. Keystrokes are debounced and the in-flight query is
 * cancelled when a newer one arrives, so typing never queues a backlog. Depends on [SearchAccess].
 */
class SearchViewModel(
    private val access: SearchAccess,
) : ViewModel() {
    private val _state = MutableStateFlow<SearchState>(SearchState.Idle)
    val state: StateFlow<SearchState> = _state.asStateFlow()

    private val _sources = MutableStateFlow<ImmutableList<Source>>(persistentListOf())
    val sources: StateFlow<ImmutableList<Source>> = _sources.asStateFlow()

    private var searchJob: Job? = null

    init {
        loadSources()
    }

    private fun loadSources() {
        viewModelScope.launch {
            _sources.value =
                try {
                    access.sources().toImmutableList()
                } catch (e: CancellationException) {
                    throw e
                } catch (_: ApiException) {
                    persistentListOf()
                }
        }
    }

    /** Schedules a debounced search for [query] and the filters, cancelling any in-flight one. */
    fun search(
        query: String,
        sourceId: Long?,
        kind: MediaKind?,
    ) {
        searchJob?.cancel()
        val trimmed = query.trim()
        if (trimmed.isEmpty()) {
            _state.value = SearchState.Idle
            return
        }
        _state.value = SearchState.Loading
        searchJob =
            viewModelScope.launch {
                delay(DEBOUNCE_MILLIS)
                run(trimmed, sourceId, kind)
            }
    }

    private suspend fun run(
        query: String,
        sourceId: Long?,
        kind: MediaKind?,
    ) {
        val result =
            try {
                val page = access.search(query, sourceId, kind, 0u, PAGE_LIMIT)
                if (page.channels.isEmpty()) {
                    SearchState.Empty
                } else {
                    SearchState.Results(
                        SearchResults(
                            channels = page.channels.toImmutableList(),
                            fuzzy = page.fuzzy,
                            context = ZapContext.Search(query, sourceId, kind),
                        ),
                    )
                }
            } catch (e: CancellationException) {
                throw e
            } catch (e: ApiException) {
                SearchState.Failed(ActionableError.from(e))
            }
        // Only publish if a newer keystroke has not superseded this search.
        if (currentCoroutineContext().isActive) _state.value = result
    }

    companion object {
        fun factory(access: SearchAccess): ViewModelProvider.Factory =
            viewModelFactory {
                initializer { SearchViewModel(access) }
            }

        private const val DEBOUNCE_MILLIS = 120L
        private const val PAGE_LIMIT = 100u
    }
}
