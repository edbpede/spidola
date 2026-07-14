// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.feature.browse

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.tv.material3.MaterialTheme
import androidx.tv.material3.Text
import dev.spidola.tv.core.corekit.PlayableChannel
import dev.spidola.tv.core.designsystem.PosterItem
import dev.spidola.tv.core.designsystem.SpidolaPalette
import dev.spidola.tv.core.designsystem.SpidolaSpacing

/** A centered informational message on the Studio canvas, for loading/empty/placeholder states. */
@Composable
internal fun CenteredMessage(message: String) {
    Box(
        modifier = Modifier.fillMaxSize().padding(SpidolaSpacing.xl),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            text = message,
            style = MaterialTheme.typography.titleLarge,
            color = SpidolaPalette.BroadcastWhite,
            textAlign = TextAlign.Center,
            modifier = Modifier.widthIn(max = 900.dp),
        )
    }
}

/** The stable poster/list/focus key for a channel: source + stable identity, refresh-proof. */
internal val PlayableChannel.key: String get() = "$sourceId-$identity"

/** Maps a channel to a home-rail poster card. */
internal fun PlayableChannel.toPoster(): PosterItem = PosterItem(id = key, title = name, subtitle = group, logo = logo)
