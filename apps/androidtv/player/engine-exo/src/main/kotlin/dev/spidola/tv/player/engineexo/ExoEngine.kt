// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

// Media3 marks its player-construction surface @UnstableApi. That marker is androidx's, not
// Kotlin's, so kotlin.OptIn does not silence it — androidx.annotation.OptIn is what Android Lint
// reads. Opting in is the whole job of this module: wrapping that surface behind the stable
// PlaybackEngine contract is what keeps the opt-in from spreading to feature code.
@file:OptIn(markerClass = [UnstableApi::class])

package dev.spidola.tv.player.engineexo

import android.content.Context
import android.util.Log
import androidx.annotation.OptIn
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.ContentScale
import androidx.media3.common.C
import androidx.media3.common.MediaItem
import androidx.media3.common.PlaybackException
import androidx.media3.common.Player
import androidx.media3.common.Timeline
import androidx.media3.common.TrackSelectionOverride
import androidx.media3.common.Tracks
import androidx.media3.common.util.UnstableApi
import androidx.media3.datasource.DefaultDataSource
import androidx.media3.datasource.DefaultHttpDataSource
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.exoplayer.source.DefaultMediaSourceFactory
import androidx.media3.session.MediaSession
import androidx.media3.ui.compose.PlayerSurface
import androidx.media3.ui.compose.SURFACE_TYPE_SURFACE_VIEW
import androidx.media3.ui.compose.modifiers.resizeWithContentScale
import androidx.media3.ui.compose.state.rememberPresentationState
import dev.spidola.tv.core.playercontract.AspectMode
import dev.spidola.tv.core.playercontract.EngineId
import dev.spidola.tv.core.playercontract.PlaybackEngine
import dev.spidola.tv.core.playercontract.PlaybackState
import dev.spidola.tv.core.playercontract.StreamRequest
import dev.spidola.tv.core.playercontract.TrackId
import dev.spidola.tv.core.playercontract.TrackSelection
import dev.spidola.tv.core.playercontract.diagnosticDetail
import dev.spidola.tv.core.playercontract.failureClass
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.getAndUpdate

/**
 * The Media3/ExoPlayer engine — the Android default (TECH_SPEC §8): the platform-integrated path,
 * with hardware decoding, the system media session, and Media3's extractors covering the HLS, DASH,
 * TS, and progressive streams the format taxonomy names.
 *
 * Construction allocates three state flows and keeps a [Context]; the `ExoPlayer` itself is built in
 * [load]. That split is not incidental. The zap path destroys and rebuilds engines constantly, so
 * construction must stay off the critical path — and the [StreamRequest] is what decides the load
 * control, the data source's headers, and the user-agent, none of which ExoPlayer lets you change
 * after `ExoPlayer.Builder.build()`. Building at [load] is therefore both the cheaper and the only
 * correct option.
 *
 * Main-thread-affine, per the contract: every method drives the player directly, and ExoPlayer
 * asserts its own thread. No coroutines are launched here — the state machine is fed by ExoPlayer's
 * listener callbacks, which arrive on the player's application thread.
 */
class ExoEngine(
    context: Context,
) : PlaybackEngine {
    override val id: EngineId = EngineId.EXOPLAYER

    private val context: Context = context.applicationContext

    private val _state = MutableStateFlow<PlaybackState>(PlaybackState.Idle)
    override val state: StateFlow<PlaybackState> = _state.asStateFlow()

    private val _tracks = MutableStateFlow(TrackSelection())
    override val tracks: StateFlow<TrackSelection> = _tracks.asStateFlow()

    private val _isSeekable = MutableStateFlow(false)
    override val isSeekable: StateFlow<Boolean> = _isSeekable.asStateFlow()

    /** Held as a flow rather than a plain field so [Surface] recomposes when [load] builds it. */
    private val player = MutableStateFlow<ExoPlayer?>(null)

    private val aspect = MutableStateFlow(AspectMode.FIT)

    private var session: MediaSession? = null

    private var released = false

    /**
     * Whether the player has ever reached `STATE_READY`. Separates the contract's two waits:
     * `STATE_BUFFERING` before it is [PlaybackState.Loading] (opening the stream), after it is
     * [PlaybackState.Buffering] (open, but starved). Keyed on readiness rather than on a rendered
     * first frame so that audio-only channels, which never render one, still leave Loading.
     */
    private var hasBeenReady = false

    private val listener =
        object : Player.Listener {
            override fun onPlaybackStateChanged(playbackState: Int) = syncState()

            override fun onIsPlayingChanged(isPlaying: Boolean) = syncState()

            override fun onPlayerError(error: PlaybackException) = fail(error)

            override fun onTracksChanged(tracks: Tracks) {
                _tracks.value = tracks.toTrackSelection()
            }

            override fun onTimelineChanged(
                timeline: Timeline,
                reason: Int,
            ) {
                _isSeekable.value = player.value?.isCurrentMediaItemSeekable == true
            }
        }

    @Composable
    override fun Surface(modifier: Modifier) {
        val active by player.collectAsState()
        val mode by aspect.collectAsState()

        Box(modifier.background(Color.Black)) {
            val current = active ?: return@Box
            val presentation = rememberPresentationState(current)
            PlayerSurface(
                player = current,
                surfaceType = SURFACE_TYPE_SURFACE_VIEW,
                modifier =
                    Modifier
                        .resizeWithContentScale(mode.contentScale, presentation.videoSizeDp)
                        .fillMaxSize(),
            )
            if (presentation.coverSurface) {
                // The surface holds the previous stream's last frame until the new one decodes;
                // on the zap path that is the outgoing channel, which would read as a glitch.
                Box(Modifier.matchParentSize().background(Color.Black))
            }
        }
    }

    override fun load(request: StreamRequest) {
        if (released) return
        check(player.value == null) {
            "ExoEngine.load is single-use (PlaybackEngine.load); dispose and rebuild to play another stream"
        }

        Log.i(PLAYBACK_TAG, "exoplayer: load ${request.logSummary()}")

        val built = buildPlayer(request)
        player.value = built
        session = openMediaSession(context, built)

        built.setMediaItem(MediaItem.fromUri(request.locator))
        built.playWhenReady = true
        built.prepare()
        transition(PlaybackState.Loading)
    }

    override fun play() {
        player.value?.play()
    }

    override fun pause() {
        player.value?.pause()
    }

    override fun seekTo(seconds: Double) {
        if (!_isSeekable.value) return
        val current = player.value ?: return
        current.seekTo((seconds.coerceAtLeast(0.0) * MILLIS_PER_SECOND).toLong())
    }

    override fun select(track: TrackId) {
        val current = player.value ?: return
        val resolved = current.resolve(track)
        if (resolved == null) {
            Log.w(PLAYBACK_TAG, "exoplayer: ignoring select for track ${track.value}, absent from the current stream")
            return
        }

        val (group, trackIndex) = resolved
        current.trackSelectionParameters =
            current.trackSelectionParameters
                .buildUpon()
                .setOverrideForType(TrackSelectionOverride(group.mediaTrackGroup, trackIndex))
                // Selecting a subtitle after clearSubtitle must undo the type-wide disable, or the
                // override would be honoured by the selector and then dropped by the renderer.
                .setTrackTypeDisabled(group.type, false)
                .build()
    }

    override fun clearSubtitle() {
        val current = player.value ?: return
        current.trackSelectionParameters =
            current.trackSelectionParameters
                .buildUpon()
                // "Off" is not a track, so it cannot be expressed as an override: the overrides are
                // dropped and the whole text type is disabled.
                .clearOverridesOfType(C.TRACK_TYPE_TEXT)
                .setTrackTypeDisabled(C.TRACK_TYPE_TEXT, true)
                .build()
    }

    override fun setAspect(mode: AspectMode) {
        aspect.value = mode
    }

    override fun release() {
        if (released) return
        released = true
        session?.release()
        session = null
        player.value?.let { current ->
            current.removeListener(listener)
            current.release()
        }
        player.value = null
        Log.i(PLAYBACK_TAG, "exoplayer: released")
    }

    private fun buildPlayer(request: StreamRequest): ExoPlayer {
        val http =
            DefaultHttpDataSource.Factory()
                // IPTV origins routinely bounce a stream between http and https across their CDN.
                .setAllowCrossProtocolRedirects(true)
                .apply {
                    request.userAgent?.let { setUserAgent(it) }
                    if (request.headers.isNotEmpty()) {
                        setDefaultRequestProperties(request.headers.associate { it.name to it.value })
                    }
                }

        // DefaultDataSource wraps the HTTP factory with the file/asset/content schemes, and
        // DefaultMediaSourceFactory picks the HLS, DASH, or progressive source from the locator.
        val sources = DefaultMediaSourceFactory(context).setDataSourceFactory(DefaultDataSource.Factory(context, http))

        return ExoPlayer
            .Builder(context)
            .setMediaSourceFactory(sources)
            .setLoadControl(request.buffering.toLoadControl())
            .build()
            .apply { addListener(listener) }
    }

    private fun syncState() {
        val current = player.value ?: return
        if (current.playbackState == Player.STATE_READY) hasBeenReady = true
        current.toPlaybackState()?.let(::transition)
    }

    private fun ExoPlayer.toPlaybackState(): PlaybackState? =
        when (playbackState) {
            Player.STATE_IDLE -> PlaybackState.Idle
            Player.STATE_BUFFERING -> if (hasBeenReady) PlaybackState.Buffering else PlaybackState.Loading
            Player.STATE_READY -> if (isPlaying) PlaybackState.Playing else PlaybackState.Paused
            Player.STATE_ENDED -> PlaybackState.Ended
            else -> null
        }

    /** The group and index [track] names, or null once a track change has invalidated the id. */
    private fun ExoPlayer.resolve(track: TrackId): Pair<Tracks.Group, Int>? {
        val coordinates = track.decode() ?: return null
        return currentTracks.groups
            .getOrNull(coordinates.groupIndex)
            ?.takeIf { coordinates.trackIndex in 0 until it.length }
            ?.to(coordinates.trackIndex)
    }

    private fun fail(error: PlaybackException) = transition(PlaybackState.Failed(error.toEngineError()))

    /**
     * The single owner of the state machine's one invariant: [PlaybackState.Failed] is terminal.
     * ExoPlayer reports `STATE_IDLE` after an error, and `load` publishes Loading after `prepare`
     * — which can fail synchronously — so without this guard a spent engine would report itself
     * live again and the shell would never offer the fallback.
     */
    private fun transition(next: PlaybackState) {
        if (released) return
        val previous = _state.getAndUpdate { current -> if (current is PlaybackState.Failed) current else next }
        if (previous == next || previous is PlaybackState.Failed) return

        when (next) {
            is PlaybackState.Failed -> {
                val detail = next.error.diagnosticDetail?.let { " — $it" }.orEmpty()
                Log.e(PLAYBACK_TAG, "exoplayer: $previous -> Failed(${next.error.failureClass})$detail")
            }
            else -> Log.i(PLAYBACK_TAG, "exoplayer: $previous -> $next")
        }
    }
}

/**
 * The contract's aspect vocabulary in Compose's terms. `PlayerSurface` has no resize mode of its
 * own — the legacy `PlayerView`'s `RESIZE_MODE_*` is a view-layer concept, and that view is not on
 * the table (§7: the surface is the Compose-native one) — so the equivalent scaling is applied to
 * the surface's own layout by `resizeWithContentScale`.
 */
private val AspectMode.contentScale: ContentScale
    get() =
        when (this) {
            AspectMode.FIT -> ContentScale.Fit
            AspectMode.FILL -> ContentScale.Crop
            AspectMode.STRETCH -> ContentScale.FillBounds
        }

private const val MILLIS_PER_SECOND = 1_000
