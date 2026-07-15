// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.feature.sources

import androidx.compose.runtime.Immutable
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import dev.spidola.tv.core.corekit.ActionableError
import dev.spidola.tv.core.corekit.PairingAccess
import dev.spidola.tv.core.corekit.PairingEvent
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import uniffi.core_api.ApiException
import uniffi.core_api.PairingSession

/** The pairing screen's phase. A closed set the screen matches exhaustively. */
@Immutable
sealed interface PairingState {
    /** Finding this TV's LAN address and opening the socket. */
    data object Starting : PairingState

    /** The server is up; [session] is the address and token to put on screen. */
    data class Ready(
        val session: PairingSession,
    ) : PairingState

    data class Failed(
        val error: ActionableError,
    ) : PairingState
}

/**
 * Runs the LAN pairing server for as long as the pairing screen is on screen (PRD §6.1).
 *
 * The server's lifetime is the security model, and this view model is what ties it to the screen:
 * [start] collects [PairingAccess.pair] in [viewModelScope], and nothing ever completes that
 * collection from the core's side, so it ends only when the scope is cancelled — which happens when
 * the screen goes away. There is no `stop()` to forget to call.
 *
 * A submission is **offered, never applied**: it goes to the [PairingHandoff] for the add-source
 * form to pre-fill, and someone confirms it there. A phone on the LAN cannot add a source to this
 * TV by itself.
 */
class PairingViewModel(
    private val access: PairingAccess,
    private val handoff: PairingHandoff,
    /** Supplies the TV's own LAN address. Injected so the view model is testable without a network:
     * the core needs a real address, but this class only needs *an* answer. */
    private val host: suspend () -> String?,
) : ViewModel() {
    private val _state = MutableStateFlow<PairingState>(PairingState.Starting)
    val state: StateFlow<PairingState> = _state.asStateFlow()

    /** Set when a phone submits, so the screen can send the viewer to the pre-filled form. */
    private val _submitted = MutableStateFlow(false)
    val submitted: StateFlow<Boolean> = _submitted.asStateFlow()

    private var pairingJob: Job? = null

    fun start() {
        if (pairingJob?.isActive == true) return
        _state.value = PairingState.Starting
        pairingJob =
            viewModelScope.launch {
                try {
                    access.pair(host()).collect { event ->
                        when (event) {
                            is PairingEvent.Started -> _state.value = PairingState.Ready(event.session)
                            is PairingEvent.Submitted -> {
                                handoff.offer(event.submission)
                                _submitted.value = true
                            }
                        }
                    }
                } catch (e: CancellationException) {
                    throw e
                } catch (e: ApiException) {
                    _state.value = PairingState.Failed(ActionableError.from(e))
                }
            }
    }

    /** Stops the server now, rather than waiting for the scope to be torn down — the viewer has
     * left the screen and the token's claim ("someone is looking at this right now") is spent. */
    fun stop() {
        pairingJob?.cancel()
        pairingJob = null
    }

    companion object {
        fun factory(
            access: PairingAccess,
            handoff: PairingHandoff,
            host: suspend () -> String? = { lanAddress() },
        ): ViewModelProvider.Factory =
            viewModelFactory {
                initializer { PairingViewModel(access, handoff, host) }
            }
    }
}
