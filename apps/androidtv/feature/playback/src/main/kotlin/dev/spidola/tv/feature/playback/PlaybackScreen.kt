// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.feature.playback

import android.provider.Settings
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.focusable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.tv.material3.MaterialTheme
import androidx.tv.material3.Text
import dev.spidola.tv.core.corekit.PlayableChannel
import dev.spidola.tv.core.corekit.PlaybackAccess
import dev.spidola.tv.core.corekit.ZapContext
import dev.spidola.tv.core.designsystem.SpidolaPalette
import dev.spidola.tv.core.designsystem.SpidolaSpacing
import dev.spidola.tv.core.playercontract.EngineRegistry
import dev.spidola.tv.core.playercontract.failure
import dev.spidola.tv.core.playercontract.isShowingVideo
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlin.time.Duration.Companion.seconds

/**
 * The playback screen: engine surface, the channel strip, and the remote mapping from PRD §8.4.
 *
 * Everything here is quiet so the strip can sing (PRD §8.1). The screen has no chrome of its own:
 * video fills it, and every control is summoned.
 */
@Composable
fun PlaybackScreen(
    channel: PlayableChannel,
    context: ZapContext,
    offset: UInt,
    access: PlaybackAccess,
    registry: EngineRegistry,
    onExit: () -> Unit,
    modifier: Modifier = Modifier,
    viewModel: PlaybackViewModel =
        viewModel(factory = PlaybackViewModel.factory(channel, context, offset, access, registry)),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val engine by viewModel.engine.collectAsStateWithLifecycle()
    val scope = rememberCoroutineScope()
    val strip = remember(scope) { StripPresenter(scope) }
    val reduceMotion = rememberReduceMotion()

    var isShowingOptions by remember { mutableStateOf(false) }
    var isShowingSeekHint by remember { mutableStateOf(false) }

    val failure = state.playback.failure
    val hasOverlay = isShowingOptions || state.fallbackOffer != null || failure != null

    // Resolved out here because `semantics {}` is not a composable scope.
    val nowPlaying = stringResource(R.string.playback_now_playing, state.channel.name)

    val rootFocus = remember { FocusRequester() }
    LaunchedEffect(hasOverlay) {
        if (!hasOverlay) rootFocus.requestFocus()
    }

    LaunchedEffect(Unit) { viewModel.start() }
    // A leaked decoder per zap is fatal on TV, so the engine dies with the composition.
    DisposableEffect(Unit) { onDispose { viewModel.release() } }

    LaunchedEffect(isShowingSeekHint) {
        if (isShowingSeekHint) {
            delay(SEEK_HINT_DWELL)
            isShowingSeekHint = false
        }
    }

    /** Back dismisses an overlay first, and only then leaves (PRD §8.4). */
    fun exit() {
        when {
            isShowingOptions -> isShowingOptions = false
            state.fallbackOffer != null -> viewModel.dismissFallback()
            strip.isVisible -> strip.dismiss()
            else -> {
                viewModel.release()
                onExit()
            }
        }
    }

    fun zap(direction: ZapDirection) {
        // The strip rides the zap: a viewer flipping channels wants to see what they landed on.
        strip.summon()
        viewModel.zap(direction)
    }

    fun seek(bySeconds: Double) {
        if (!state.isSeekable) {
            // "No-op with hint" (PRD §8.4) — a live stream cannot seek, and silence would read as a
            // broken remote.
            isShowingSeekHint = true
            strip.summon()
            return
        }
        viewModel.seek(bySeconds)
        strip.summon()
    }

    Box(
        modifier =
            modifier
                .fillMaxSize()
                .background(SpidolaPalette.Studio)
                // The video itself cannot be announced, and D-pad zapping is invisible to a screen
                // reader that is never told about it (PRD §6.10).
                .semantics { contentDescription = nowPlaying }
                .focusRequester(rootFocus)
                .focusable()
                .onPreviewKeyEvent { event ->
                    if (event.type != KeyEventType.KeyDown) return@onPreviewKeyEvent false
                    if (event.key == Key.Back) {
                        exit()
                        return@onPreviewKeyEvent true
                    }
                    // An overlay owns the D-pad while it is up, so its buttons stay reachable.
                    if (hasOverlay) return@onPreviewKeyEvent false
                    when (event.key) {
                        Key.DirectionUp -> zap(ZapDirection.PREVIOUS)
                        Key.DirectionDown -> zap(ZapDirection.NEXT)
                        Key.DirectionLeft -> seek(-SEEK_STEP_SECONDS)
                        Key.DirectionRight -> seek(SEEK_STEP_SECONDS)
                        Key.DirectionCenter, Key.Enter -> strip.summon()
                        Key.MediaPlayPause, Key.MediaPlay, Key.MediaPause -> viewModel.togglePause()
                        Key.Menu -> isShowingOptions = true
                        else -> return@onPreviewKeyEvent false
                    }
                    true
                },
    ) {
        engine?.Surface(Modifier.fillMaxSize())

        if (!state.playback.isShowingVideo && state.fallbackOffer == null && failure == null) {
            LoadingTreatment(channelName = state.channel.name)
        }

        val offer = state.fallbackOffer
        when {
            offer != null ->
                FallbackOfferView(
                    offer = offer,
                    onTry = viewModel::tryOtherPlayer,
                    onBack = ::exit,
                )

            // A failure with no other engine to offer still has to say what happened and what to
            // press (PRD §6.3) — an error with no action is a design bug.
            failure != null ->
                PlaybackErrorView(error = failure, onRetry = viewModel::start, onBack = ::exit)

            isShowingOptions ->
                PlaybackOptionsView(
                    tracks = state.tracks,
                    aspect = state.aspect,
                    onSelect = viewModel::select,
                    onClearSubtitle = viewModel::clearSubtitle,
                    onCycleAspect = viewModel::cycleAspect,
                )

            else ->
                StripLayer(
                    visible = strip.isVisible,
                    reduceMotion = reduceMotion,
                    state = state,
                    showSeekHint = isShowingSeekHint,
                )
        }
    }
}

/** The strip slides up (PRD §8.5), under 200 ms and suppressed under reduce-motion (§8.6). */
@Composable
private fun BoxScope.StripLayer(
    visible: Boolean,
    reduceMotion: Boolean,
    state: PlaybackUiState,
    showSeekHint: Boolean,
) {
    AnimatedVisibility(
        visible = visible,
        modifier = Modifier.align(Alignment.BottomStart),
        enter =
            if (reduceMotion) {
                fadeIn(tween(STRIP_MOTION_MILLIS))
            } else {
                slideInVertically(tween(STRIP_MOTION_MILLIS)) { it } + fadeIn(tween(STRIP_MOTION_MILLIS))
            },
        exit =
            if (reduceMotion) {
                fadeOut(tween(STRIP_MOTION_MILLIS))
            } else {
                slideOutVertically(tween(STRIP_MOTION_MILLIS)) { it } + fadeOut(tween(STRIP_MOTION_MILLIS))
            },
    ) {
        Column(modifier = Modifier.fillMaxWidth()) {
            ChannelStrip(
                window = state.window,
                channel = state.channel,
                isLive = state.channel.isLive,
            )
            if (showSeekHint) {
                Box(modifier = Modifier.fillMaxWidth().padding(top = SpidolaSpacing.s)) {
                    SeekHintView()
                }
            }
            Box(modifier = Modifier.padding(bottom = SpidolaSpacing.safeVertical))
        }
    }
}

/** Shown only while there is no video. The strip and the error surfaces own everything after. */
@Composable
private fun BoxScope.LoadingTreatment(channelName: String) {
    Column(
        modifier = Modifier.align(Alignment.Center),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(SpidolaSpacing.m),
    ) {
        Text(
            text = channelName,
            style = MaterialTheme.typography.titleLarge,
            color = SpidolaPalette.BroadcastWhite,
        )
        Text(
            text = stringResource(R.string.playback_tuning),
            style = MaterialTheme.typography.labelMedium,
            color = SpidolaPalette.TestCardAmber,
        )
    }
}

/**
 * Owns the strip's visibility and its self-dismiss timer.
 *
 * Its own type because "summon, then dismiss unless summoned again" is a small state machine, and
 * leaving it inline in the composable would put a cancellable job in a value recreated on every
 * recomposition.
 */
@Stable
class StripPresenter(
    private val scope: CoroutineScope,
) {
    var isVisible: Boolean by mutableStateOf(false)
        private set

    private var timeout: Job? = null

    /**
     * Shows the strip and restarts its timer. Re-summoning while visible extends it, so a viewer
     * zapping steadily never has the strip vanish mid-flip.
     */
    fun summon() {
        isVisible = true
        timeout?.cancel()
        timeout =
            scope.launch {
                delay(DWELL)
                isVisible = false
            }
    }

    fun dismiss() {
        timeout?.cancel()
        timeout = null
        isVisible = false
    }

    private companion object {
        /**
         * Long enough to read a channel name and glance at the neighbours; short enough that it never
         * feels like chrome the viewer has to dismiss.
         */
        val DWELL = 5.seconds
    }
}

/**
 * Whether the system asks for motion to be suppressed (PRD §8.6). Android expresses this as the
 * animator duration scale, which the accessibility "remove animations" setting drives to zero.
 */
@Composable
private fun rememberReduceMotion(): Boolean {
    val context = LocalContext.current
    return remember(context) {
        Settings.Global.getFloat(
            context.contentResolver,
            Settings.Global.ANIMATOR_DURATION_SCALE,
            1f,
        ) == 0f
    }
}

private const val SEEK_STEP_SECONDS = 10.0
private const val STRIP_MOTION_MILLIS = 180
private val SEEK_HINT_DWELL = 2.seconds
