// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.core.corekit

import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Test
import uniffi.core_api.ApiException
import uniffi.core_api.InputField
import uniffi.core_api.InputIssue

class ActionableErrorTest {
    @Test
    fun `structured input codes become shell-owned copy`() {
        val exception = ApiException.InvalidInput(InputField.ADDRESS, InputIssue.INVALID)

        val error = ActionableError.from(exception)

        assertEquals(R.string.core_error_issue_invalid, error.message.resource)
        assertEquals(R.string.core_error_field_address, error.message.argument)
    }

    @Test
    fun `all input codes retain a recovery action`() {
        InputField.entries.forEach { field ->
            InputIssue.entries.forEach { issue ->
                val error = ActionableError.from(ApiException.InvalidInput(field, issue))

                assertEquals(ErrorAction.FIX_INPUT, error.primaryAction)
            }
        }
    }
}
