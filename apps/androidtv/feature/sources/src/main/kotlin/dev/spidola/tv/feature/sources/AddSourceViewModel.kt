// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.feature.sources

import android.util.Log
import androidx.annotation.StringRes
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import dev.spidola.tv.core.corekit.ActionableError
import dev.spidola.tv.core.corekit.ImportEvent
import dev.spidola.tv.core.corekit.SourcesAccess
import dev.spidola.tv.core.corekit.id
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Job
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.core_api.ApiException
import uniffi.core_api.ImportOutcome
import uniffi.core_api.ImportStage
import uniffi.core_api.Source

/** Which add-source flow the form drives. Android has no cross-app file picker for arbitrary text
 * on TV in the M1 scope, so a local playlist is added by pasting its text; the URL flow fetches and
 * streams it (TECH_SPEC §4.5). */
enum class AddSourceMode(
    @param:StringRes val title: Int,
) {
    URL(R.string.add_source_mode_url),
    FILE(R.string.add_source_mode_file),
    XTREAM(R.string.add_source_mode_xtream),
}

/**
 * The immutable form the screen owns and hands to [AddSourceViewModel.submit].
 *
 * Credential-bearing fields are in flight to the core and nowhere else: the form holds them only
 * between the keystroke and the submit, and the screen keeps them out of saved state (TECH_SPEC §12).
 */
data class AddSourceForm(
    val mode: AddSourceMode,
    val name: String,
    val url: String,
    val content: String,
    val userAgent: String,
    val acceptInvalidTls: Boolean,
    val server: String = "",
    val username: String = "",
    val password: String = "",
)

/**
 * Values a phone handed the TV over LAN pairing, ready for someone to confirm (PRD §6.1). Pairing
 * fills the add-source form; it never adds a source on its own.
 *
 * Deliberately not a `NavKey` payload: an Xtream submission carries a password, and the Navigation 3
 * back stack is serialized into saved instance state, which would write that credential to disk.
 * This travels in memory only ([PairingHandoff]).
 */
data class AddSourcePrefill(
    val mode: AddSourceMode,
    val url: String = "",
    val server: String = "",
    val username: String = "",
    val password: String = "",
)

/** The add-source screen's phase. A closed set the screen matches exhaustively. */
sealed interface AddSourceState {
    data object Editing : AddSourceState

    data class Importing(
        val stage: ImportStage,
        val channels: ULong,
    ) : AddSourceState

    data class Done(
        val outcome: ImportOutcome,
    ) : AddSourceState

    data class Failed(
        val error: ActionableError,
    ) : AddSourceState
}

/**
 * Drives adding a source and importing its catalog with live progress, cancellation, and a
 * diagnostics summary (PRD §6.1). Depends on the narrow [SourcesAccess]; unit-tested against a
 * fake. A cancelled or failed first import deletes the just-created empty source, so a half-added
 * source never litters the list.
 */
class AddSourceViewModel(
    private val access: SourcesAccess,
) : ViewModel() {
    private val _state = MutableStateFlow<AddSourceState>(AddSourceState.Editing)
    val state: StateFlow<AddSourceState> = _state.asStateFlow()

    private val _validation = MutableStateFlow<String?>(null)
    val validation: StateFlow<String?> = _validation.asStateFlow()

    private var importJob: Job? = null

    fun submit(form: AddSourceForm) {
        val problem = validate(form)
        if (problem != null) {
            _validation.value = problem
            return
        }
        _validation.value = null
        _state.value = AddSourceState.Importing(ImportStage.CONNECTING, 0uL)
        importJob = viewModelScope.launch { runImport(form) }
    }

    /** Cancels the running import; the stream terminates, cancelling the core task at its next batch
     * boundary, and the just-created source is removed. */
    fun cancel() {
        importJob?.cancel()
    }

    private fun validate(form: AddSourceForm): String? {
        if (form.name.isBlank()) return "Give this source a name."
        return when (form.mode) {
            AddSourceMode.URL -> if (form.url.isBlank()) "Enter the playlist address." else null
            AddSourceMode.FILE -> if (form.content.isBlank()) "Paste the playlist text." else null
            AddSourceMode.XTREAM -> validateXtream(form)
        }
    }

    /** Only checks for blanks. Whether the account *works* is the core's answer, not a guess made
     * here: `addXtream` verifies against the headend before storing, so a wrong password comes back
     * as an actionable error rather than being waved through by a shell-side regex. */
    private fun validateXtream(form: AddSourceForm): String? =
        when {
            form.server.isBlank() -> "Enter the server address."
            form.username.isBlank() -> "Enter your username."
            form.password.isBlank() -> "Enter your password."
            else -> null
        }

    private suspend fun runImport(form: AddSourceForm) {
        val created =
            try {
                createSource(form)
            } catch (e: CancellationException) {
                _state.value = AddSourceState.Editing
                throw e
            } catch (e: ApiException) {
                _state.value = AddSourceState.Failed(ActionableError.from(e))
                return
            }

        val flow =
            when (form.mode) {
                // An Xtream account is added verified but empty, exactly like an M3U-by-URL source:
                // the catalog arrives on a refresh, which is the same streamed import with the same
                // progress and the same cancellation.
                AddSourceMode.URL, AddSourceMode.XTREAM -> access.importUrl(created.id)
                AddSourceMode.FILE -> access.importContent(created.id, form.content)
            }

        var imported = false
        var failure: ActionableError? = null
        try {
            flow.collect { event ->
                when (event) {
                    is ImportEvent.Progress ->
                        _state.value =
                            AddSourceState.Importing(event.progress.stage, event.progress.channelsSeen)
                    is ImportEvent.Complete -> {
                        _state.value = AddSourceState.Done(event.outcome)
                        imported = true
                    }
                    is ImportEvent.Failed ->
                        if (event.error !is ApiException.Cancelled) {
                            failure = ActionableError.from(event.error)
                        }
                }
            }
        } finally {
            if (!imported) {
                // Cancelled or failed: only a completed import earns the source its row, so drop the
                // empty one we just created. Settling the state after the delete keeps a fast retry
                // from racing the cleanup.
                withContext(NonCancellable) { deleteQuietly(created.id) }
                _state.value = failure?.let { AddSourceState.Failed(it) } ?: AddSourceState.Editing
            }
        }
    }

    private suspend fun createSource(form: AddSourceForm): Source =
        when (form.mode) {
            AddSourceMode.URL ->
                access.addM3uUrl(
                    name = form.name.trim(),
                    url = form.url.trim(),
                    userAgent = form.userAgent.trim().ifBlank { null },
                    acceptInvalidTls = form.acceptInvalidTls,
                )
            AddSourceMode.FILE -> access.addM3uFile(form.name.trim())
            // The password goes straight through to the core, which verifies the account and hands
            // the credential to the platform secure store. It is not trimmed: leading or trailing
            // space can be part of a password, and silently "fixing" one would reject a valid
            // account with a message blaming the user.
            AddSourceMode.XTREAM ->
                access.addXtream(
                    name = form.name.trim(),
                    server = form.server.trim(),
                    username = form.username.trim(),
                    password = form.password,
                )
        }

    private suspend fun deleteQuietly(id: Long) {
        try {
            access.deleteSource(id)
        } catch (e: ApiException) {
            Log.w(LOG_TAG, "cleanup of abandoned source failed", e)
        }
    }

    companion object {
        fun factory(access: SourcesAccess): ViewModelProvider.Factory =
            viewModelFactory {
                initializer { AddSourceViewModel(access) }
            }

        private const val LOG_TAG = "spidola::sources"
    }
}
