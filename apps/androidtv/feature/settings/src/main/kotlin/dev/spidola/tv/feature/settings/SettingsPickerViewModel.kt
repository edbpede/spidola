// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.feature.settings

import androidx.compose.runtime.Immutable
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import dev.spidola.tv.core.corekit.ActionableError
import dev.spidola.tv.core.corekit.SettingsAccess
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import uniffi.core_api.ApiException

/**
 * The picker screen's state machine. One flow, not a load-state plus an "applied" flag beside it:
 * the screen returns exactly when the write lands, and two flows could disagree about whether it
 * had.
 */
@Immutable
sealed interface PickerState {
    data object Loading : PickerState

    /** Showing the options; [snapshot] carries the value to mark as selected. */
    data class Choosing(
        val snapshot: SettingsSnapshot,
    ) : PickerState

    /** The choice is written. The screen returns to the settings list. */
    data object Applied : PickerState

    data class Failed(
        val error: ActionableError,
    ) : PickerState
}

/**
 * Backs one option-picker screen, for whichever [SettingsPicker] it was opened with — one screen
 * serves all nine closed-set settings (PRD §6.9).
 *
 * The write is a single exhaustive `when` over [SettingValue]: every setting the picker can carry
 * has a branch, and a setting added to the vocabulary breaks this build until it has one. There is
 * no string id to parse and nothing to `valueOf`, so a picker cannot be opened for a setting it
 * cannot write.
 */
class SettingsPickerViewModel(
    private val access: SettingsAccess,
    val picker: SettingsPicker,
) : ViewModel() {
    private val _state = MutableStateFlow<PickerState>(PickerState.Loading)
    val state: StateFlow<PickerState> = _state.asStateFlow()

    fun load() {
        viewModelScope.launch {
            _state.value =
                try {
                    PickerState.Choosing(SettingsSnapshot.of(access.settings()))
                } catch (e: CancellationException) {
                    throw e
                } catch (e: ApiException) {
                    PickerState.Failed(ActionableError.from(e))
                }
        }
    }

    /** Writes [value], then reports [PickerState.Applied] so the screen returns to the list. */
    fun choose(value: SettingValue) {
        viewModelScope.launch {
            _state.value =
                try {
                    write(value)
                    PickerState.Applied
                } catch (e: CancellationException) {
                    throw e
                } catch (e: ApiException) {
                    PickerState.Failed(ActionableError.from(e))
                }
        }
    }

    private suspend fun write(value: SettingValue) =
        when (value) {
            is SettingValue.DefaultEngine -> access.setDefaultEngine(value.engine?.value)
            is SettingValue.Buffering -> access.setBuffering(value.profile)
            is SettingValue.SubtitleGlyphSize -> access.setSubtitleSize(value.size)
            is SettingValue.SubtitlePlate -> access.setSubtitleBackground(value.background)
            is SettingValue.Language -> access.setLanguage(value.choice.tag)
            is SettingValue.Density -> access.setDensity(value.density)
            is SettingValue.RecentsRetention -> access.setRecentsRetentionDays(value.days)
            is SettingValue.ImageCache -> access.setImageCacheMaxMb(value.megabytes)
            is SettingValue.Logging -> access.setLogLevel(value.level)
        }

    companion object {
        fun factory(
            access: SettingsAccess,
            picker: SettingsPicker,
        ): ViewModelProvider.Factory =
            viewModelFactory {
                initializer { SettingsPickerViewModel(access, picker) }
            }
    }
}
