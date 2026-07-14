// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.feature.sources

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import dev.spidola.tv.core.corekit.ActionableError
import dev.spidola.tv.core.corekit.ImportEvent
import dev.spidola.tv.core.corekit.LoadState
import dev.spidola.tv.core.corekit.SourcesAccess
import dev.spidola.tv.core.corekit.id
import dev.spidola.tv.core.corekit.isRefreshable
import dev.spidola.tv.core.corekit.name
import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.toImmutableList
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import uniffi.core_api.ApiException
import uniffi.core_api.Source

/** An auto-refresh interval offered in the sources UI (PRD §6.1 per-source auto-refresh). */
enum class AutoRefreshOption(
    val seconds: UInt?,
    val label: String,
) {
    OFF(null, "Manual only"),
    HOURLY(3600u, "Every hour"),
    SIX_HOURLY(21_600u, "Every 6 hours"),
    DAILY(86_400u, "Every day"),
    ;

    /** The next option in the cycle, so a single click steps through the intervals. */
    fun next(): AutoRefreshOption = entries[(ordinal + 1) % entries.size]

    companion object {
        fun from(seconds: UInt?): AutoRefreshOption = entries.firstOrNull { it.seconds == seconds } ?: OFF
    }
}

/**
 * Backs the manage-sources screen: the list plus rename / enable-disable / refresh / delete /
 * auto-refresh (PRD §6.1). Refresh preserves favorites and hidden flags via the core's stable
 * identity (§4.4), so the shell need do nothing special. Depends on the narrow [SourcesAccess].
 */
class SourcesViewModel(
    private val access: SourcesAccess,
) : ViewModel() {
    private val _state = MutableStateFlow<LoadState<ImmutableList<Source>>>(LoadState.Loading)
    val state: StateFlow<LoadState<ImmutableList<Source>>> = _state.asStateFlow()

    private val _refreshing = MutableStateFlow<Set<Long>>(emptySet())
    val refreshing: StateFlow<Set<Long>> = _refreshing.asStateFlow()

    private val _status = MutableStateFlow<String?>(null)
    val status: StateFlow<String?> = _status.asStateFlow()

    init {
        load()
    }

    fun load() {
        viewModelScope.launch {
            _state.value =
                try {
                    val sources = access.sources()
                    if (sources.isEmpty()) LoadState.Empty else LoadState.Ready(sources.toImmutableList())
                } catch (e: CancellationException) {
                    throw e
                } catch (e: ApiException) {
                    LoadState.Failed(ActionableError.from(e))
                }
        }
    }

    fun rename(
        id: Long,
        name: String,
    ) {
        if (name.isBlank()) return
        mutate { access.rename(id, name.trim()) }
    }

    fun setEnabled(
        id: Long,
        enabled: Boolean,
    ) = mutate { access.setEnabled(id, enabled) }

    fun setAutoRefresh(
        id: Long,
        option: AutoRefreshOption,
    ) = mutate { access.setAutoRefresh(id, option.seconds) }

    fun delete(id: Long) = mutate { access.deleteSource(id) }

    fun refresh(source: Source) {
        if (!source.isRefreshable) {
            _status.value = "This source was added from a file — re-add it to update its channels."
            return
        }
        viewModelScope.launch {
            _refreshing.update { it + source.id }
            try {
                access.importUrl(source.id).collect { event ->
                    when (event) {
                        is ImportEvent.Progress -> Unit
                        is ImportEvent.Complete ->
                            _status.value = "Refreshed ${source.name}: ${event.outcome.inserted} channels"
                        is ImportEvent.Failed ->
                            if (event.error !is ApiException.Cancelled) {
                                _status.value = ActionableError.from(event.error).message
                            }
                    }
                }
            } finally {
                _refreshing.update { it - source.id }
            }
            load()
        }
    }

    /** Runs a mutating action, surfacing any failure as a status message and reloading on success —
     * so the UI always reflects the core, the single source of truth. */
    private fun mutate(action: suspend () -> Unit) {
        viewModelScope.launch {
            try {
                action()
                _status.value = null
                load()
            } catch (e: CancellationException) {
                throw e
            } catch (e: ApiException) {
                _status.value = ActionableError.from(e).message
            }
        }
    }

    companion object {
        fun factory(access: SourcesAccess): ViewModelProvider.Factory =
            viewModelFactory {
                initializer { SourcesViewModel(access) }
            }
    }
}
