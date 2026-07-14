// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.feature.search

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import dev.spidola.tv.core.corekit.ActionableError
import dev.spidola.tv.core.corekit.ErrorAction
import dev.spidola.tv.core.designsystem.ActionableErrorView
import dev.spidola.tv.core.designsystem.SpidolaErrorButton

/**
 * Renders a corekit [ActionableError] through the designsystem [ActionableErrorView], wiring each
 * prescribed action to a handler (PRD §6.3). Owned by the slice because it joins the core error
 * model to the visual component, which neither horizontal layer depends on.
 */
@Composable
fun ActionableErrorContent(
    error: ActionableError,
    onRetry: () -> Unit,
    onGoBack: () -> Unit,
    modifier: Modifier = Modifier,
    onFixInput: (() -> Unit)? = null,
) {
    fun button(action: ErrorAction): SpidolaErrorButton =
        SpidolaErrorButton(
            title = action.label,
            onClick =
                when (action) {
                    ErrorAction.RETRY -> onRetry
                    ErrorAction.GO_BACK -> onGoBack
                    ErrorAction.FIX_INPUT -> onFixInput ?: onGoBack
                },
        )
    ActionableErrorView(
        failureClass = error.failureClass,
        message = error.message,
        primary = button(error.primaryAction),
        others = error.otherActions.map(::button),
        modifier = modifier,
    )
}
