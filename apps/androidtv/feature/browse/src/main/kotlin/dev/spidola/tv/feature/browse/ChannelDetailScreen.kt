// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.feature.browse

import android.net.Uri
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.tv.material3.MaterialTheme
import androidx.tv.material3.Text
import dev.spidola.tv.core.corekit.BrowseAccess
import dev.spidola.tv.core.corekit.PlayableChannel
import dev.spidola.tv.core.designsystem.LogoImage
import dev.spidola.tv.core.designsystem.SpidolaPalette
import dev.spidola.tv.core.designsystem.SpidolaRow
import dev.spidola.tv.core.designsystem.SpidolaSpacing

/**
 * The channel detail screen: artwork, name, group, and the actions a household member reaches for —
 * Play (records a recent; the engine lands in Phase 5), favorite, and hide. This is the D-pad-first
 * equivalent of the browse context menu.
 */
@Composable
fun ChannelDetailScreen(
    channel: PlayableChannel,
    access: BrowseAccess,
    modifier: Modifier = Modifier,
    viewModel: ChannelDetailViewModel =
        viewModel(factory = ChannelDetailViewModel.factory(channel, access)),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val playFocus = remember { FocusRequester() }
    LaunchedEffect(Unit) { playFocus.requestFocus() }

    Row(
        modifier =
            modifier
                .fillMaxSize()
                .background(SpidolaPalette.Studio)
                .padding(SpidolaSpacing.xl),
        horizontalArrangement = Arrangement.spacedBy(SpidolaSpacing.xl),
    ) {
        LogoImage(
            url = channel.logo,
            modifier = Modifier.width(420.dp).aspectRatio(DETAIL_LOGO_ASPECT),
        )
        Column(verticalArrangement = Arrangement.spacedBy(SpidolaSpacing.m)) {
            Text(
                text = channel.name,
                style = MaterialTheme.typography.displayLarge,
                color = SpidolaPalette.BroadcastWhite,
            )
            channel.group?.let {
                Text(text = it, style = MaterialTheme.typography.bodyLarge, color = SpidolaPalette.Static)
            }
            Text(
                text = Uri.parse(channel.locator).host ?: channel.locator,
                style = MaterialTheme.typography.labelMedium,
                color = SpidolaPalette.Static,
            )
            SpidolaRow(
                title = "Play",
                onClick = viewModel::play,
                modifier = Modifier.focusRequester(playFocus).testTag("detail-play"),
            )
            SpidolaRow(
                title = if (state.isFavorite) "Remove favorite" else "Add favorite",
                onClick = viewModel::toggleFavorite,
                modifier = Modifier.testTag("detail-favorite"),
            )
            SpidolaRow(
                title = if (state.isHidden) "Unhide" else "Hide",
                onClick = viewModel::toggleHidden,
                modifier = Modifier.testTag("detail-hide"),
            )
            state.notice?.let {
                Text(text = it, style = MaterialTheme.typography.labelMedium, color = SpidolaPalette.TestCardAmber)
            }
        }
    }
}

private const val DETAIL_LOGO_ASPECT = 16f / 9f
