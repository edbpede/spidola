// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.core.corekit

import android.content.Context
import uniffi.core_api.ApiException
import uniffi.core_api.InputField
import uniffi.core_api.InputIssue

/**
 * A prescribed user action for an error (PRD §6.3). The set is deliberately small; the shell
 * renders each as a focusable button.
 */
enum class ErrorAction(
    val label: Int,
) {
    RETRY(R.string.core_action_retry),
    GO_BACK(R.string.core_action_go_back),
    FIX_INPUT(R.string.core_action_edit),
}

/** A shell-owned string resource with an optional shell-owned string argument. */
data class LocalizedText(
    val resource: Int,
    val argument: Int? = null,
) {
    /** Resolves the resource only at the Android presentation edge. */
    fun resolve(context: Context): String =
        argument?.let { context.getString(resource, context.getString(it)) } ?: context.getString(resource)
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
    val failureClass: LocalizedText,
    val message: LocalizedText,
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
                        LocalizedText(R.string.core_error_network_title),
                        LocalizedText(R.string.core_error_network_message),
                        ErrorAction.RETRY,
                        listOf(ErrorAction.GO_BACK),
                    )
                is ApiException.Timeout ->
                    ActionableError(
                        LocalizedText(R.string.core_error_timeout_title),
                        LocalizedText(R.string.core_error_timeout_message),
                        ErrorAction.RETRY,
                        listOf(ErrorAction.GO_BACK),
                    )
                is ApiException.Unauthorized ->
                    ActionableError(
                        LocalizedText(R.string.core_error_unauthorized_title),
                        LocalizedText(R.string.core_error_unauthorized_message),
                        ErrorAction.FIX_INPUT,
                        listOf(ErrorAction.GO_BACK),
                    )
                is ApiException.NotFound ->
                    ActionableError(
                        LocalizedText(R.string.core_error_not_found_title),
                        LocalizedText(R.string.core_error_not_found_message),
                        ErrorAction.GO_BACK,
                        emptyList(),
                    )
                is ApiException.InvalidInput ->
                    ActionableError(
                        LocalizedText(R.string.core_error_invalid_title),
                        inputMessage(error.field, error.issue),
                        ErrorAction.FIX_INPUT,
                        listOf(ErrorAction.GO_BACK),
                    )
                is ApiException.ParseFailed ->
                    ActionableError(
                        LocalizedText(R.string.core_error_parse_title),
                        LocalizedText(R.string.core_error_parse_message),
                        ErrorAction.RETRY,
                        listOf(ErrorAction.GO_BACK),
                    )
                is ApiException.StorageCorrupt ->
                    ActionableError(
                        LocalizedText(R.string.core_error_storage_title),
                        LocalizedText(R.string.core_error_storage_message),
                        ErrorAction.RETRY,
                        listOf(ErrorAction.GO_BACK),
                    )
                is ApiException.Cancelled ->
                    ActionableError(
                        LocalizedText(R.string.core_error_cancelled_title),
                        LocalizedText(R.string.core_error_cancelled_message),
                        ErrorAction.GO_BACK,
                        emptyList(),
                    )
                is ApiException.Internal ->
                    ActionableError(
                        LocalizedText(R.string.core_error_internal_title),
                        LocalizedText(R.string.core_error_internal_message),
                        ErrorAction.RETRY,
                        listOf(ErrorAction.GO_BACK),
                    )
            }

        /** Turns stable boundary codes into shell-owned copy. The generated exception's message is
         * intentionally ignored because it is diagnostic data, not localized UI prose. */
        private fun inputMessage(
            field: InputField,
            issue: InputIssue,
        ): LocalizedText {
            val fieldResource =
                when (field) {
                    InputField.ADDRESS -> R.string.core_error_field_address
                    InputField.SERVER -> R.string.core_error_field_server
                    InputField.NAME -> R.string.core_error_field_name
                    InputField.HEADER -> R.string.core_error_field_header
                    InputField.LOG_LEVEL -> R.string.core_error_field_log_level
                    InputField.FILE -> R.string.core_error_field_file
                    InputField.SOURCE -> R.string.core_error_field_source
                }
            val messageResource =
                when (issue) {
                    InputIssue.EMPTY -> R.string.core_error_issue_empty
                    InputIssue.INVALID -> R.string.core_error_issue_invalid
                    InputIssue.UNSUPPORTED -> R.string.core_error_issue_unsupported
                    InputIssue.UNAVAILABLE -> R.string.core_error_issue_unavailable
                }
            return LocalizedText(messageResource, fieldResource)
        }
    }
}
