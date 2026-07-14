// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.core.corekit

import uniffi.core_api.ApiException

/**
 * A prescribed user action for an error (PRD §6.3). The set is deliberately small; the shell
 * renders each as a focusable button.
 */
enum class ErrorAction(
    val label: String,
) {
    RETRY("Try again"),
    GO_BACK("Go back"),
    FIX_INPUT("Edit"),
}

/**
 * The plain-language presentation of an [ApiException]: a short failure class, a one-sentence
 * message, and a **non-empty** set of prescribed actions (PRD §6.3, mirroring the core's
 * `ApiError::ux` table in `crates/core-api/src/error.rs`). Diagnostic detail stays in the log
 * stream, never here (PRD §8.6).
 *
 * "No action available" is unrepresentable: [primaryAction] is a single required value, so every
 * `ActionableError` carries at least one action by construction — a UI that renders one can never
 * be handed an actionless error.
 */
data class ActionableError(
    val failureClass: String,
    val message: String,
    val primaryAction: ErrorAction,
    val otherActions: List<ErrorAction>,
) {
    /** Every offered action, primary first — always non-empty. */
    val actions: List<ErrorAction> get() = listOf(primaryAction) + otherActions

    companion object {
        /**
         * Maps a boundary [ApiException] onto its presentation. The `when` is exhaustive over the
         * sealed error hierarchy; a variant added to the core forces a decision here at compile
         * time (TECH_SPEC §5).
         */
        fun from(error: ApiException): ActionableError =
            when (error) {
                is ApiException.NetworkUnreachable ->
                    ActionableError(
                        "Can't reach the source",
                        "Spidola couldn't connect. Check the address and your network, then try again.",
                        ErrorAction.RETRY,
                        listOf(ErrorAction.GO_BACK),
                    )
                is ApiException.Timeout ->
                    ActionableError(
                        "The source is slow to respond",
                        "The source didn't answer in time. It may be busy — try again in a moment.",
                        ErrorAction.RETRY,
                        listOf(ErrorAction.GO_BACK),
                    )
                is ApiException.Unauthorized ->
                    ActionableError(
                        "Login was rejected",
                        "The source didn't accept these sign-in details. Edit them and try again.",
                        ErrorAction.FIX_INPUT,
                        listOf(ErrorAction.GO_BACK),
                    )
                is ApiException.NotFound ->
                    ActionableError(
                        "Not available anymore",
                        "This isn't at the source any longer.",
                        ErrorAction.GO_BACK,
                        emptyList(),
                    )
                is ApiException.InvalidInput ->
                    ActionableError(
                        "That entry isn't valid",
                        error.reason,
                        ErrorAction.FIX_INPUT,
                        listOf(ErrorAction.GO_BACK),
                    )
                is ApiException.ParseFailed ->
                    ActionableError(
                        "No channels found",
                        "Spidola reached the source but found no channels to add. " +
                            "Check the playlist and try again.",
                        ErrorAction.RETRY,
                        listOf(ErrorAction.GO_BACK),
                    )
                is ApiException.StorageCorrupt ->
                    ActionableError(
                        "Local storage problem",
                        "Something went wrong saving to this device. Try again.",
                        ErrorAction.RETRY,
                        listOf(ErrorAction.GO_BACK),
                    )
                is ApiException.Cancelled ->
                    ActionableError(
                        "Cancelled",
                        "That was cancelled.",
                        ErrorAction.GO_BACK,
                        emptyList(),
                    )
                is ApiException.Internal ->
                    ActionableError(
                        "Something went wrong",
                        "An unexpected problem came up. Try again.",
                        ErrorAction.RETRY,
                        listOf(ErrorAction.GO_BACK),
                    )
            }
    }
}
