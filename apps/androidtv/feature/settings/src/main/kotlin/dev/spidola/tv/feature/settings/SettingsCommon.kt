// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.feature.settings

import androidx.annotation.StringRes
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.tv.material3.MaterialTheme
import androidx.tv.material3.Text
import dev.spidola.tv.core.designsystem.SpidolaPalette
import dev.spidola.tv.core.designsystem.SpidolaSpacing

// The two pieces of chrome every screen in this slice shares. Both the settings list and the
// diagnostics screen are section-headed lists on the Studio canvas, so the header and the
// placeholder live here rather than as a copy on each screen.

/** A section heading. Marked as a heading so a screen reader can jump between sections. */
internal fun LazyListScope.section(
    @StringRes title: Int,
) = item(key = "section-$title") {
    Text(
        text = stringResource(title),
        style = MaterialTheme.typography.titleLarge,
        color = SpidolaPalette.BroadcastWhite,
        modifier =
            Modifier
                .padding(top = SpidolaSpacing.m, bottom = SpidolaSpacing.xs)
                .semantics { heading() },
    )
}

/** A placeholder message on the Studio canvas, for the moment before a screen has its data. */
@Composable
internal fun Centered(message: String) {
    Box(modifier = Modifier.fillMaxSize().padding(SpidolaSpacing.xl)) {
        Text(text = message, style = MaterialTheme.typography.titleLarge, color = SpidolaPalette.BroadcastWhite)
    }
}
