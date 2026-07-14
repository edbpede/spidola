// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

@file:Suppress("MatchingDeclarationName") // Named for the public composable; the data type is a helper.

package dev.spidola.tv.core.designsystem

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.tv.material3.ClickableSurfaceDefaults
import androidx.tv.material3.MaterialTheme
import androidx.tv.material3.Surface
import androidx.tv.material3.Text

/** One labelled action button for an actionable error. */
data class SpidolaErrorButton(
    val title: String,
    val onClick: () -> Unit,
)

/**
 * Presents a failure with its plain-language class, a one-sentence message, and a **non-empty** set
 * of actions (PRD §6.3). "No action available" is unrepresentable: [primary] is a single required
 * button, so the view always offers at least one thing to do — an error dead-end can never be
 * rendered. The primary button takes initial focus.
 */
@Composable
fun ActionableErrorView(
    failureClass: String,
    message: String,
    primary: SpidolaErrorButton,
    modifier: Modifier = Modifier,
    others: List<SpidolaErrorButton> = emptyList(),
) {
    val primaryFocus = remember { FocusRequester() }
    LaunchedEffect(Unit) { primaryFocus.requestFocus() }
    Column(
        modifier = modifier.fillMaxSize().padding(SpidolaSpacing.xl),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(SpidolaSpacing.l, Alignment.CenterVertically),
    ) {
        Text(
            text = failureClass,
            style = MaterialTheme.typography.titleLarge,
            color = SpidolaPalette.BroadcastWhite,
            textAlign = TextAlign.Center,
        )
        Text(
            text = message,
            style = MaterialTheme.typography.bodyLarge,
            color = SpidolaPalette.Static,
            textAlign = TextAlign.Center,
            modifier = Modifier.widthIn(max = 900.dp),
        )
        Row(horizontalArrangement = Arrangement.spacedBy(SpidolaSpacing.m)) {
            ErrorButton(primary, isPrimary = true, modifier = Modifier.focusRequester(primaryFocus))
            others.forEach { button -> ErrorButton(button, isPrimary = false) }
        }
    }
}

@Composable
private fun ErrorButton(
    button: SpidolaErrorButton,
    isPrimary: Boolean,
    modifier: Modifier = Modifier,
) {
    Surface(
        onClick = button.onClick,
        modifier = modifier,
        shape = ClickableSurfaceDefaults.shape(shape = SpidolaFocus.cardShape),
        colors =
            ClickableSurfaceDefaults.colors(
                containerColor = if (isPrimary) SpidolaPalette.TestCardAmber else SpidolaPalette.Set,
                contentColor = if (isPrimary) SpidolaPalette.Studio else SpidolaPalette.BroadcastWhite,
            ),
        scale = SpidolaFocus.scale(),
        border = SpidolaFocus.border(),
    ) {
        Text(
            text = button.title,
            style = MaterialTheme.typography.bodyLarge,
            modifier = Modifier.padding(horizontal = SpidolaSpacing.l, vertical = SpidolaSpacing.m),
        )
    }
}
