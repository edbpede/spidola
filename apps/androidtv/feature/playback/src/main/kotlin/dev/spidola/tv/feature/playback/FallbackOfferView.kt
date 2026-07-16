// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.feature.playback

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.tv.material3.ClickableSurfaceDefaults
import androidx.tv.material3.MaterialTheme
import androidx.tv.material3.Surface
import androidx.tv.material3.Text
import dev.spidola.tv.core.designsystem.ActionableErrorView
import dev.spidola.tv.core.designsystem.SpidolaErrorButton
import dev.spidola.tv.core.designsystem.SpidolaFocus
import dev.spidola.tv.core.designsystem.SpidolaPalette
import dev.spidola.tv.core.designsystem.SpidolaSpacing
import dev.spidola.tv.core.playercontract.EngineError
import dev.spidola.tv.core.playercontract.failureClass
import dev.spidola.tv.core.playercontract.message

/**
 * The loud fallback (TECH_SPEC §8): the engine failed in a way another engine could plausibly
 * survive, so the viewer is *offered* the swap and chooses. Nothing switches on its own.
 *
 * The remember toggle is the difference between a one-off rescue and a channel that simply works
 * from now on — a channel whose format only one engine handles is a permanent fact about that
 * channel, and making the viewer re-answer nightly would be the bug.
 */
@Composable
fun FallbackOfferView(
    offer: FallbackOffer,
    onTry: (Boolean) -> Unit,
    onBack: () -> Unit,
    modifier: Modifier = Modifier,
    canRemember: Boolean = true,
) {
    var rememberChoice by rememberSaveable(canRemember) { mutableStateOf(canRemember) }
    val tryOther = remember { FocusRequester() }
    LaunchedEffect(Unit) { tryOther.requestFocus() }

    Box(
        modifier = modifier.fillMaxSize().padding(SpidolaSpacing.xl),
        contentAlignment = Alignment.Center,
    ) {
        Column(
            modifier =
                Modifier
                    .clip(SpidolaFocus.cardShape)
                    .background(SpidolaPalette.Set)
                    .padding(SpidolaSpacing.xl),
            verticalArrangement = Arrangement.spacedBy(SpidolaSpacing.m),
        ) {
            Text(
                text = offer.error.failureClass,
                style = MaterialTheme.typography.titleLarge,
                color = SpidolaPalette.BroadcastWhite,
            )
            Text(
                text = offer.error.message,
                style = MaterialTheme.typography.bodyLarge,
                color = SpidolaPalette.Static,
            )
            Row(
                modifier = Modifier.padding(top = SpidolaSpacing.s),
                horizontalArrangement = Arrangement.spacedBy(SpidolaSpacing.m),
            ) {
                OfferButton(
                    title = stringResource(R.string.playback_fallback_try_other),
                    isPrimary = true,
                    onClick = { onTry(rememberChoice) },
                    modifier = Modifier.focusRequester(tryOther),
                )
                // Its two neighbours name what pressing them does, and so does this one: the title is
                // the choice it would switch to, and the choice in force is the button's state. Named
                // the other way round it is a coin toss — someone arriving on "Remember for this
                // channel" cannot hear whether that is the setting or the offer, and guessing wrong
                // switches remembering off on the way to choosing it, which the primary button then
                // carries out (PRD §6.10).
                if (canRemember) {
                    val rememberState =
                        stringResource(
                            if (rememberChoice) {
                                R.string.playback_fallback_remember_state
                            } else {
                                R.string.playback_fallback_once_state
                            },
                        )
                    OfferButton(
                        title =
                            stringResource(
                                if (rememberChoice) {
                                    R.string.playback_fallback_once
                                } else {
                                    R.string.playback_fallback_remember
                                },
                            ),
                        isPrimary = false,
                        onClick = { rememberChoice = !rememberChoice },
                        modifier = Modifier.semantics { stateDescription = rememberState },
                    )
                }
                OfferButton(
                    title = stringResource(R.string.playback_fallback_back),
                    isPrimary = false,
                    onClick = onBack,
                )
            }
        }
    }
}

@Composable
private fun OfferButton(
    title: String,
    isPrimary: Boolean,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Surface(
        onClick = onClick,
        modifier = modifier,
        shape = ClickableSurfaceDefaults.shape(shape = SpidolaFocus.cardShape),
        colors =
            ClickableSurfaceDefaults.colors(
                containerColor = if (isPrimary) SpidolaPalette.TestCardAmber else SpidolaPalette.Studio,
                contentColor = if (isPrimary) SpidolaPalette.Studio else SpidolaPalette.BroadcastWhite,
            ),
        scale = SpidolaFocus.scale(),
        border = SpidolaFocus.border(),
    ) {
        Text(
            text = title,
            style = MaterialTheme.typography.bodyLarge,
            modifier = Modifier.padding(horizontal = SpidolaSpacing.l, vertical = SpidolaSpacing.m),
        )
    }
}

/**
 * A playback failure with no other engine to offer. Still says what happened and what to press next
 * — an error with no action is a design bug (PRD §6.3).
 */
@Composable
fun PlaybackErrorView(
    error: EngineError,
    onRetry: () -> Unit,
    onBack: () -> Unit,
    modifier: Modifier = Modifier,
) {
    ActionableErrorView(
        failureClass = error.failureClass,
        message = error.message,
        primary = SpidolaErrorButton(title = "Try again", onClick = onRetry),
        modifier = modifier,
        others = listOf(SpidolaErrorButton(title = "Go back", onClick = onBack)),
    )
}

/**
 * Shown when left/right is pressed on a stream that cannot seek (PRD §8.4: "no-op with hint").
 * Silence would read as a broken remote.
 */
@Composable
fun SeekHintView(modifier: Modifier = Modifier) {
    Box(modifier = modifier.fillMaxWidth(), contentAlignment = Alignment.Center) {
        Text(
            text = stringResource(R.string.playback_seek_hint),
            style = MaterialTheme.typography.labelMedium,
            color = SpidolaPalette.Static,
            modifier =
                Modifier
                    .clip(HintShape)
                    .background(SpidolaPalette.Set.copy(alpha = HINT_ALPHA))
                    .padding(horizontal = SpidolaSpacing.m, vertical = SpidolaSpacing.s),
        )
    }
}

private val HintShape = RoundedCornerShape(percent = 50)
private const val HINT_ALPHA = 0.9f
