// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.feature.settings

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import dev.spidola.tv.core.corekit.ActionableError
import dev.spidola.tv.core.corekit.LoadState
import dev.spidola.tv.core.corekit.SettingsAccess
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import uniffi.core_api.ApiException

/**
 * A transient outcome of an action on the settings list. Typed rather than a message, so the words
 * stay in `strings.xml` and the view model stays translatable and `Context`-free (PRD §6.10).
 */
sealed interface SettingsStatus {
    data object HistoryCleared : SettingsStatus

    data class Failed(
        val error: ActionableError,
    ) : SettingsStatus
}

/**
 * Backs the settings list: reads the whole snapshot, and owns the two actions that are not pickers —
 * the recently-watched off-switch and clear-history (PRD §6.9).
 *
 * The off-switch and clear route through [SettingsAccess] to the core's **recents** service, not its
 * settings service, which owns that flag; the snapshot only reports it. So a toggle here writes
 * where the rest of the app reads, and the home screen's own toggle cannot disagree with this one.
 *
 * Loading is driven by the screen rather than an `init` block, because this view model outlives a
 * trip to a picker screen: the picker writes a setting and pops, and the list must re-read rather
 * than show what it loaded before the change.
 */
class SettingsViewModel(
    private val access: SettingsAccess,
) : ViewModel() {
    private val _state = MutableStateFlow<LoadState<SettingsSnapshot>>(LoadState.Loading)
    val state: StateFlow<LoadState<SettingsSnapshot>> = _state.asStateFlow()

    private val _status = MutableStateFlow<SettingsStatus?>(null)
    val status: StateFlow<SettingsStatus?> = _status.asStateFlow()

    fun load() {
        viewModelScope.launch {
            _state.value =
                try {
                    LoadState.Ready(SettingsSnapshot.of(access.settings()))
                } catch (e: CancellationException) {
                    throw e
                } catch (e: ApiException) {
                    LoadState.Failed(ActionableError.from(e))
                }
        }
    }

    fun setRecentsEnabled(enabled: Boolean) = mutate { access.setRecentsEnabled(enabled) }

    fun clearHistory() = mutate(onDone = SettingsStatus.HistoryCleared) { access.clearRecents() }

    /** Runs a mutating action, surfacing failure as an actionable status and re-reading the core on
     * success — so the list always shows what the core holds, never what the shell hoped it wrote. */
    private fun mutate(
        onDone: SettingsStatus? = null,
        action: suspend () -> Unit,
    ) {
        viewModelScope.launch {
            try {
                action()
                _status.value = onDone
                load()
            } catch (e: CancellationException) {
                throw e
            } catch (e: ApiException) {
                _status.value = SettingsStatus.Failed(ActionableError.from(e))
            }
        }
    }

    companion object {
        fun factory(access: SettingsAccess): ViewModelProvider.Factory =
            viewModelFactory {
                initializer { SettingsViewModel(access) }
            }
    }
}
