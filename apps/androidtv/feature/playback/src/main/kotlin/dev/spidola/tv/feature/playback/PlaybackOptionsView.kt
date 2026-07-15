// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.feature.playback

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.unit.dp
import androidx.tv.material3.ClickableSurfaceDefaults
import androidx.tv.material3.MaterialTheme
import androidx.tv.material3.Surface
import androidx.tv.material3.Text
import dev.spidola.tv.core.designsystem.SpidolaFocus
import dev.spidola.tv.core.designsystem.SpidolaPalette
import dev.spidola.tv.core.designsystem.SpidolaSpacing
import dev.spidola.tv.core.playercontract.AspectMode
import dev.spidola.tv.core.playercontract.MediaTrack
import dev.spidola.tv.core.playercontract.TrackId
import dev.spidola.tv.core.playercontract.TrackKind
import dev.spidola.tv.core.playercontract.TrackSelection

/**
 * Playback options, summoned by the menu button (PRD §8.4): audio, subtitles, and aspect.
 *
 * The vocabulary is engine-neutral by construction — every option here routes through the contract,
 * so the viewer never learns which decoder is running (TECH_SPEC §8). Engine choice itself is not
 * offered here: it is a per-channel decision the loud fallback already asks about at the only moment
 * it means anything, and a menu entry would invite fiddling with something the app is supposed to
 * get right on its own.
 */
@Composable
fun PlaybackOptionsView(
    tracks: TrackSelection,
    aspect: AspectMode,
    onSelect: (TrackId) -> Unit,
    onClearSubtitle: () -> Unit,
    onCycleAspect: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Box(modifier = modifier.fillMaxSize(), contentAlignment = Alignment.CenterEnd) {
        Column(
            modifier =
                Modifier
                    .width(PANEL_WIDTH)
                    .fillMaxHeight()
                    .background(SpidolaPalette.Set)
                    .padding(SpidolaSpacing.xl),
            verticalArrangement = Arrangement.spacedBy(SpidolaSpacing.l, Alignment.CenterVertically),
        ) {
            val audio = tracks.tracksOf(TrackKind.AUDIO)
            Section(stringResource(R.string.playback_options_audio)) {
                if (audio.isEmpty()) {
                    EmptyRow(stringResource(R.string.playback_options_single_audio))
                } else {
                    audio.forEach { track ->
                        OptionRow(
                            title = label(track),
                            isOn = tracks.selectedAudio == track.id,
                            onClick = { onSelect(track.id) },
                        )
                    }
                }
            }

            Section(stringResource(R.string.playback_options_subtitles)) {
                OptionRow(
                    title = stringResource(R.string.playback_options_subtitles_off),
                    isOn = tracks.selectedSubtitle == null,
                    onClick = onClearSubtitle,
                )
                tracks.tracksOf(TrackKind.SUBTITLE).forEach { track ->
                    OptionRow(
                        title = label(track),
                        isOn = tracks.selectedSubtitle == track.id,
                        onClick = { onSelect(track.id) },
                    )
                }
            }

            Section(stringResource(R.string.playback_options_picture)) {
                OptionRow(title = aspect.label(), isOn = false, onClick = onCycleAspect)
            }
        }
    }
}

@Composable
private fun Section(
    title: String,
    content: @Composable () -> Unit,
) {
    Column(verticalArrangement = Arrangement.spacedBy(SpidolaSpacing.s)) {
        Text(
            text = title,
            style = MaterialTheme.typography.labelMedium,
            color = SpidolaPalette.Static,
        )
        content()
    }
}

/**
 * One option, checked when it is the one in use. The check announces itself as the row's state
 * rather than as a glyph, since a screen reader that reads "✓" aloud names the ornament and not the
 * fact (PRD §6.10). Only a checked row has state to report: the picture row cycles rather than
 * chooses and is never on, so "not selected" would answer a question it never asked.
 */
@Composable
private fun OptionRow(
    title: String,
    isOn: Boolean,
    onClick: () -> Unit,
) {
    val selected = stringResource(R.string.playback_options_selected)
    Surface(
        onClick = onClick,
        modifier =
            Modifier
                .fillMaxWidth()
                .then(if (isOn) Modifier.semantics { stateDescription = selected } else Modifier),
        shape = ClickableSurfaceDefaults.shape(shape = SpidolaFocus.cardShape),
        colors =
            ClickableSurfaceDefaults.colors(
                containerColor = SpidolaPalette.Studio,
                contentColor = SpidolaPalette.BroadcastWhite,
            ),
        scale = SpidolaFocus.scale(),
        border = SpidolaFocus.border(),
    ) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(SpidolaSpacing.m),
            horizontalArrangement = Arrangement.spacedBy(SpidolaSpacing.s),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(text = title, style = MaterialTheme.typography.bodyLarge, modifier = Modifier.weight(1f))
            if (isOn) {
                Text(
                    text = "✓",
                    style = MaterialTheme.typography.bodyLarge,
                    color = SpidolaPalette.TestCardAmber,
                    modifier = Modifier.clearAndSetSemantics { },
                )
            }
        }
    }
}

@Composable
private fun EmptyRow(text: String) {
    Text(
        text = text,
        style = MaterialTheme.typography.bodyLarge,
        color = SpidolaPalette.Static,
        modifier = Modifier.padding(SpidolaSpacing.m),
    )
}

/** Prefers the stream's language tag, since "English" beats "Track 2" from the couch. */
@Composable
private fun label(track: MediaTrack): String {
    val language = track.language
    if (!language.isNullOrEmpty()) {
        if (track.label.isEmpty()) return language
        return stringResource(R.string.playback_track_label, track.label, language)
    }
    return track.label
}

private val PANEL_WIDTH = 640.dp
