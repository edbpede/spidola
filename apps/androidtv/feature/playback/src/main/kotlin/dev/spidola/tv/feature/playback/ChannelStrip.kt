// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.feature.playback

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.tv.material3.MaterialTheme
import androidx.tv.material3.Text
import dev.spidola.tv.core.corekit.PlayableChannel
import dev.spidola.tv.core.corekit.ZapWindow
import dev.spidola.tv.core.designsystem.LogoImage
import dev.spidola.tv.core.designsystem.SmpteRibbon
import dev.spidola.tv.core.designsystem.SpidolaPalette
import dev.spidola.tv.core.designsystem.SpidolaSpacing
import uniffi.core_api.MediaKind

/**
 * The signature (PRD §8.5): a broadcast lower-third that slides up over live video, showing the
 * playing channel with its neighbours peeking above and below for zap-ahead browsing, underlined by
 * a three-point ribbon of the SMPTE bar spectrum.
 *
 * The peek is the whole point — it is what makes the strip a *zapping* instrument rather than a
 * caption. The viewer sees where up and down go before pressing, which is how a broadcast tuner has
 * always behaved.
 *
 * It renders from state it is handed and starts no work: it must appear in one frame and never stall
 * video (PRD §8.5).
 */
@Composable
fun ChannelStrip(
    window: ZapWindow?,
    channel: PlayableChannel,
    isLive: Boolean,
    modifier: Modifier = Modifier,
) {
    // Both resolved out here: `semantics {}` is not a composable scope, and the band and the
    // announcement must name the same position rather than each compute one.
    val position = position(window)
    val description = accessibilityLabel(channel, isLive, position)
    Column(
        // A lower-third sits on the lower third. The video above it stays uncovered, which is the
        // difference between a strip and a scrim.
        modifier =
            modifier
                .fillMaxWidth()
                .background(SpidolaPalette.Set.copy(alpha = BAND_ALPHA))
                .semantics(mergeDescendants = true) { contentDescription = description },
    ) {
        Peek(window?.previous, edge = PeekEdge.TOP)
        Band(channel = channel, isLive = isLive, position = position)
        SmpteRibbon()
        Peek(window?.next, edge = PeekEdge.BOTTOM)
    }
}

/**
 * The band: logo, name, and the live marker. Now/next EPG joins it in Phase 8 — the row is laid out
 * to take it without moving anything that is already here.
 */
@Composable
private fun Band(
    channel: PlayableChannel,
    isLive: Boolean,
    position: String?,
) {
    Row(
        modifier =
            Modifier
                .fillMaxWidth()
                .padding(horizontal = SpidolaSpacing.xl, vertical = SpidolaSpacing.m),
        horizontalArrangement = Arrangement.spacedBy(SpidolaSpacing.m),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        LogoImage(
            url = channel.logo,
            modifier = Modifier.width(LOGO_WIDTH).aspectRatio(LOGO_ASPECT),
        )
        Column(
            modifier = Modifier.weight(1f),
            verticalArrangement = Arrangement.spacedBy(SpidolaSpacing.xs),
        ) {
            Text(
                text = channel.name,
                style = MaterialTheme.typography.titleLarge,
                color = SpidolaPalette.BroadcastWhite,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            channel.group?.let { group ->
                Text(
                    text = group,
                    style = MaterialTheme.typography.labelMedium,
                    color = SpidolaPalette.Static,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
        if (isLive) {
            LiveMarker()
        }
        if (position != null) {
            Text(
                text = position,
                style = MaterialTheme.typography.labelMedium,
                color = SpidolaPalette.Static,
            )
        }
    }
}

private enum class PeekEdge { TOP, BOTTOM }

/**
 * An adjacent channel, dimmed and half-height: legible enough to aim at, quiet enough that the
 * playing channel stays the subject. Decoration for the zap, so it is cleared from the accessibility
 * tree — the strip announces itself as one element.
 */
@Composable
private fun Peek(
    neighbour: PlayableChannel?,
    edge: PeekEdge,
) {
    if (neighbour == null) return
    Row(
        modifier =
            Modifier
                .fillMaxWidth()
                .background(SpidolaPalette.Studio.copy(alpha = PEEK_ALPHA))
                .padding(horizontal = SpidolaSpacing.xl, vertical = SpidolaSpacing.xs)
                .clearAndSetSemantics { },
        horizontalArrangement = Arrangement.spacedBy(SpidolaSpacing.s),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            text = if (edge == PeekEdge.TOP) "▲" else "▼",
            style = MaterialTheme.typography.labelMedium,
            color = SpidolaPalette.Static,
        )
        Text(
            text = neighbour.name,
            style = MaterialTheme.typography.labelMedium,
            color = SpidolaPalette.Static,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

/** The live indicator — one of exactly three things Test-Card Amber is allowed to mark (PRD §8.2). */
@Composable
private fun LiveMarker() {
    Row(
        horizontalArrangement = Arrangement.spacedBy(SpidolaSpacing.xs),
        verticalAlignment = Alignment.CenterVertically,
        modifier = Modifier.clearAndSetSemantics { },
    ) {
        Box(
            modifier =
                Modifier
                    .width(DOT_SIZE)
                    .aspectRatio(1f)
                    .background(SpidolaPalette.TestCardAmber, shape = CircleShape),
        )
        Text(
            text = stringResource(R.string.playback_live_marker),
            style = MaterialTheme.typography.labelMedium.copy(letterSpacing = LIVE_TRACKING),
            color = SpidolaPalette.TestCardAmber,
        )
    }
}

/**
 * Position in the ring, shown only when the ring's length is known — a search ring is paged without
 * a count, and "3 of ?" is worse than nothing.
 */
@Composable
private fun position(window: ZapWindow?): String? {
    val total = window?.total ?: return null
    return stringResource(R.string.playback_position, (window.offset + 1u).toInt(), total.toInt())
}

@Composable
private fun accessibilityLabel(
    channel: PlayableChannel,
    isLive: Boolean,
    position: String?,
): String {
    val live = stringResource(R.string.playback_live_announcement)
    return buildList {
        add(channel.name)
        channel.group?.let(::add)
        if (isLive) add(live)
        position?.let(::add)
    }.joinToString(", ")
}

/** The live marker is only honest for live channels; a movie has no "LIVE", and a recent carries no
 * kind at all, so nothing is claimed without evidence. */
internal val PlayableChannel.isLive: Boolean
    get() = kind == MediaKind.LIVE

private val LOGO_WIDTH = 120.dp

/** Channel logos are broadcast-shaped; the tile reserves 16:9 so a missing logo holds the row. */
private const val LOGO_ASPECT = 16f / 9f
private val DOT_SIZE = 8.dp
private val LIVE_TRACKING = 1.5.sp

/** The band is translucent so the video reads through it — a lower-third, not a panel. */
private const val BAND_ALPHA = 0.92f
private const val PEEK_ALPHA = 0.75f
