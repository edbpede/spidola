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
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.tv.material3.MaterialTheme
import androidx.tv.material3.Text
import dev.spidola.tv.core.corekit.BrowseAccess
import dev.spidola.tv.core.corekit.EpgAccess
import dev.spidola.tv.core.corekit.PlayableChannel
import dev.spidola.tv.core.designsystem.LogoImage
import dev.spidola.tv.core.designsystem.ScheduleTape
import dev.spidola.tv.core.designsystem.SpidolaPalette
import dev.spidola.tv.core.designsystem.SpidolaRow
import dev.spidola.tv.core.designsystem.SpidolaSpacing
import java.text.DateFormat
import java.util.Date

/**
 * The channel detail screen: artwork, name, group, and the actions a household member reaches for —
 * Play, favorite, and hide. This is the D-pad-first equivalent of the browse context menu.
 *
 * Play is a navigation intent, so the slice announces it through [onPlay] and the shell decides
 * where it goes — which keeps this screen free of the app's route types and of the playback slice
 * (doctrine §3.1).
 */
@Composable
fun ChannelDetailScreen(
    channel: PlayableChannel,
    access: BrowseAccess,
    epgAccess: EpgAccess,
    onPlay: () -> Unit,
    modifier: Modifier = Modifier,
    viewModel: ChannelDetailViewModel =
        viewModel(factory = ChannelDetailViewModel.factory(channel, access, epgAccess)),
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
            val schedule = state.schedule
            ScheduleTape(
                currentLabel = stringResource(R.string.browse_schedule_now),
                nextLabel = stringResource(R.string.browse_schedule_next),
                currentTime = schedule?.current?.startUnix?.asTime(),
                currentTitle = schedule?.current?.title,
                nextTime = schedule?.next?.startUnix?.asTime(),
                nextTitle = schedule?.next?.title,
                unavailable = stringResource(R.string.browse_schedule_unavailable),
                modifier = Modifier.width(620.dp).testTag("detail-schedule"),
            )
            SpidolaRow(
                title = stringResource(R.string.browse_detail_play),
                onClick = onPlay,
                modifier = Modifier.focusRequester(playFocus).testTag("detail-play"),
            )
            // Both rows label themselves with the verb, so on their own they leave the current state
            // to be inferred from it — and "Remove favorite" is a slower way to learn "Favorite" than
            // being told. Naming the state separately keeps the label about the press (PRD §6.10).
            val favoriteState =
                stringResource(
                    if (state.isFavorite) {
                        R.string.browse_detail_favorite_state
                    } else {
                        R.string.browse_detail_not_favorite_state
                    },
                )
            SpidolaRow(
                title =
                    stringResource(
                        if (state.isFavorite) {
                            R.string.browse_detail_remove_favorite
                        } else {
                            R.string.browse_detail_add_favorite
                        },
                    ),
                onClick = viewModel::toggleFavorite,
                modifier =
                    Modifier
                        .semantics { stateDescription = favoriteState }
                        .testTag("detail-favorite"),
            )
            val hiddenState =
                stringResource(
                    if (state.isHidden) R.string.browse_detail_hidden_state else R.string.browse_detail_visible_state,
                )
            SpidolaRow(
                title =
                    stringResource(
                        if (state.isHidden) R.string.browse_detail_show else R.string.browse_detail_hide,
                    ),
                onClick = viewModel::toggleHidden,
                modifier =
                    Modifier
                        .semantics { stateDescription = hiddenState }
                        .testTag("detail-hide"),
            )
            state.notice?.let {
                Text(
                    text = it.message.resolve(LocalContext.current),
                    style = MaterialTheme.typography.labelMedium,
                    color = SpidolaPalette.TestCardAmber,
                )
            }
        }
    }
}

private const val DETAIL_LOGO_ASPECT = 16f / 9f
private const val UNIX_MILLIS_PER_SECOND = 1_000L

private fun Long.asTime(): String {
    return DateFormat.getTimeInstance(DateFormat.SHORT).format(Date(this * UNIX_MILLIS_PER_SECOND))
}
