// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

@file:Suppress("MatchingDeclarationName") // Groups the browse slice's navigation + error bridge.

package dev.spidola.tv.feature.browse

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import dev.spidola.tv.core.corekit.ActionableError
import dev.spidola.tv.core.corekit.ErrorAction
import dev.spidola.tv.core.corekit.PlayableChannel
import dev.spidola.tv.core.designsystem.ActionableErrorView
import dev.spidola.tv.core.designsystem.SpidolaErrorButton
import uniffi.core_api.MediaKind

/**
 * The browse slice's navigation intents, wired by the app's composition root to the Navigation 3
 * back stack (TECH_SPEC §7). The slice depends on these lambdas, never on the app's route types, so
 * it stays free of sideways/upward dependencies (doctrine §3.1).
 */
data class BrowseNavigator(
    val openSource: (id: Long, name: String) -> Unit,
    val openChannels: (sourceId: Long, kind: MediaKind, group: String?, title: String) -> Unit,
    val openChannel: (PlayableChannel) -> Unit,
    val openSearch: () -> Unit,
    val manageSources: () -> Unit,
)

/**
 * Renders a corekit [ActionableError] through the designsystem [ActionableErrorView], wiring each
 * prescribed action to a handler (PRD §6.3). The bridge lives at the feature layer because it joins
 * the core error model (corekit) to the visual component (designsystem), and neither horizontal
 * layer should depend on the other.
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
