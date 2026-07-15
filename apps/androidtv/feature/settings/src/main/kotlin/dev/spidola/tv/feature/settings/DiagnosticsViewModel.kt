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
import dev.spidola.tv.core.corekit.LoadState
import dev.spidola.tv.core.corekit.SettingsAccess
import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.toImmutableList
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import uniffi.core_api.ApiException
import uniffi.core_api.LogLevel

/**
 * The build a support thread is actually talking about (PRD §6.9). The app version comes from the
 * composition root's `BuildConfig`; the rest comes from the core's startup handshake, so the two
 * halves of the app can never be reported as one version they don't share.
 */
@Immutable
data class DiagnosticsVersions(
    val app: String,
    val core: String,
    val coreRevision: String,
    val schema: UInt,
    val boundary: UInt,
)

/** Everything the diagnostics screen shows: the current log level, the recent log lines, and the
 * versions block. */
@Immutable
data class DiagnosticsReport(
    val logLevel: LogLevel,
    val activity: ImmutableList<String>,
    val versions: DiagnosticsVersions,
)

/**
 * Backs the diagnostics screen (PRD §6.9): the recorded-detail level, a viewer over the core's
 * recent log lines, and the versions block.
 *
 * The activity viewer shows the lines on screen rather than exporting them to a file, keeping parity
 * with tvOS, which has no user-visible file system (PRD §7). The core's export is a blocking FFI
 * read; [SettingsAccess] does that hop off the main thread, so this view model just asks.
 */
class DiagnosticsViewModel(
    private val access: SettingsAccess,
    private val appVersion: String,
) : ViewModel() {
    private val _state = MutableStateFlow<LoadState<DiagnosticsReport>>(LoadState.Loading)
    val state: StateFlow<LoadState<DiagnosticsReport>> = _state.asStateFlow()

    fun load() {
        viewModelScope.launch {
            _state.value =
                try {
                    val handshake = access.handshake()
                    LoadState.Ready(
                        DiagnosticsReport(
                            logLevel = access.settings().logLevel,
                            activity = access.exportLogs().toImmutableList(),
                            versions =
                                DiagnosticsVersions(
                                    app = appVersion,
                                    core = handshake.coreVersion,
                                    coreRevision = handshake.coreGitRevision,
                                    schema = handshake.schemaVersion,
                                    boundary = handshake.boundaryVersion,
                                ),
                        ),
                    )
                } catch (e: CancellationException) {
                    throw e
                } catch (e: ApiException) {
                    LoadState.Failed(ActionableError.from(e))
                }
        }
    }

    companion object {
        fun factory(
            access: SettingsAccess,
            appVersion: String,
        ): ViewModelProvider.Factory =
            viewModelFactory {
                initializer { DiagnosticsViewModel(access, appVersion) }
            }
    }
}
