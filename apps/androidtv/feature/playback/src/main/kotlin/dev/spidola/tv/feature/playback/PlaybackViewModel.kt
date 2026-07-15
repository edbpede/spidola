// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.feature.playback

import android.util.Log
import androidx.compose.runtime.Immutable
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import dev.spidola.tv.core.corekit.PlayableChannel
import dev.spidola.tv.core.corekit.PlaybackAccess
import dev.spidola.tv.core.corekit.ZapContext
import dev.spidola.tv.core.corekit.ZapWindow
import dev.spidola.tv.core.playercontract.AspectMode
import dev.spidola.tv.core.playercontract.BufferingProfile
import dev.spidola.tv.core.playercontract.EngineError
import dev.spidola.tv.core.playercontract.EngineId
import dev.spidola.tv.core.playercontract.EngineRegistry
import dev.spidola.tv.core.playercontract.EngineSelection
import dev.spidola.tv.core.playercontract.PlaybackEngine
import dev.spidola.tv.core.playercontract.PlaybackState
import dev.spidola.tv.core.playercontract.StreamRequest
import dev.spidola.tv.core.playercontract.TrackId
import dev.spidola.tv.core.playercontract.TrackSelection
import dev.spidola.tv.core.playercontract.diagnosticDetail
import dev.spidola.tv.core.playercontract.failure
import dev.spidola.tv.core.playercontract.failureClass
import dev.spidola.tv.core.playercontract.isShowingVideo
import dev.spidola.tv.core.playercontract.offersOtherPlayer
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import uniffi.core_api.ApiException
import kotlin.time.Duration.Companion.milliseconds
import kotlin.time.TimeSource

/**
 * The offer shown when the default engine hit a format/decode failure (TECH_SPEC §8).
 *
 * Fallback is **loud, never silent**: this value exists so the viewer chooses, and its presence is
 * the only way an engine ever changes mid-channel. A silent swap would make engine bugs invisible.
 */
data class FallbackOffer(
    val error: EngineError,
    /** The engine "Try other player" would use. */
    val alternate: EngineId,
)

/** Which way D-pad up/down zaps (PRD §8.4). */
enum class ZapDirection {
    PREVIOUS,
    NEXT,
    ;

    internal fun channel(window: ZapWindow?): PlayableChannel? =
        when (this) {
            PREVIOUS -> window?.previous
            NEXT -> window?.next
        }

    internal fun offset(from: UInt): UInt =
        when (this) {
            PREVIOUS -> if (from == 0u) 0u else from - 1u
            NEXT -> from + 1u
        }
}

/** The playback screen's observable state. The live engine is held separately, so this stays a
 * stable, immutable value the Compose compiler can skip on. */
@Immutable
data class PlaybackUiState(
    val channel: PlayableChannel,
    val playback: PlaybackState = PlaybackState.Idle,
    /** The playing channel and its neighbours — the channel strip's peek and the zap ends. */
    val window: ZapWindow? = null,
    val tracks: TrackSelection = TrackSelection(),
    val isSeekable: Boolean = false,
    val aspect: AspectMode = AspectMode.FIT,
    val fallbackOffer: FallbackOffer? = null,
    /**
     * Set when the resolved engine could not be built — a composition bug, surfaced honestly rather
     * than as a blank screen.
     */
    val engineUnavailable: Boolean = false,
)

/**
 * Backs the playback screen (TECH_SPEC §6: view models are unit-tested against a fake corekit and
 * the contract's `FakeEngine`).
 *
 * It owns the engine's whole life: resolve by policy → load → run → dispose. Zapping disposes and
 * rebuilds, because engines are single-use by contract and that is the path the channel-zapper
 * persona lives in (PRD §8.5) — so it is kept free of anything that could stall a rebuild.
 */
class PlaybackViewModel(
    channel: PlayableChannel,
    private val context: ZapContext,
    offset: UInt,
    private val access: PlaybackAccess,
    private val registry: EngineRegistry,
) : ViewModel() {
    private val _state = MutableStateFlow(PlaybackUiState(channel = channel))
    val state: StateFlow<PlaybackUiState> = _state.asStateFlow()

    /** The live engine. The screen hosts its surface; nothing else reaches through it. */
    private val _engine = MutableStateFlow<PlaybackEngine?>(null)
    val engine: StateFlow<PlaybackEngine?> = _engine.asStateFlow()

    private var offset: UInt = offset
    private var engineJob: Job? = null
    private var loadStartedAt: TimeSource.Monotonic.ValueTimeMark? = null

    /**
     * Resolves the engine by policy and starts the stream. The recents record and the zap window are
     * deliberately not awaited before `load`: click-to-first-frame is budgeted at two seconds
     * (PRD §9) and neither is needed to put video on screen.
     */
    fun start() {
        viewModelScope.launch {
            play(_state.value.channel, engineOverride = null)
            launch { loadWindow() }
            launch { recordRecent() }
        }
    }

    /**
     * Zaps to an adjacent channel — the sacred path (TECH_SPEC §11). Tears the engine down and
     * rebuilds, which is exactly what the contract's single-use engines are designed for.
     */
    fun zap(direction: ZapDirection) {
        val target = direction.channel(_state.value.window) ?: return
        offset = direction.offset(offset)
        _state.update { it.copy(channel = target) }
        viewModelScope.launch {
            // The window is refreshed after the stream is loading, not before: the peek is cosmetic
            // and must never sit between a D-pad press and video.
            play(target, engineOverride = null)
            launch { loadWindow() }
            launch { recordRecent() }
        }
    }

    /** Accepts the loud-fallback offer, optionally remembering the choice for this channel. */
    fun tryOtherPlayer(remember: Boolean) {
        val offer = _state.value.fallbackOffer ?: return
        _state.update { it.copy(fallbackOffer = null) }
        viewModelScope.launch {
            if (remember) {
                rememberEngine(offer.alternate)
            }
            play(_state.value.channel, engineOverride = offer.alternate)
        }
    }

    fun dismissFallback() {
        _state.update { it.copy(fallbackOffer = null) }
    }

    // region Transport

    fun togglePause() {
        val engine = _engine.value ?: return
        when (_state.value.playback) {
            PlaybackState.Playing, PlaybackState.Buffering -> engine.pause()
            PlaybackState.Paused -> engine.play()
            PlaybackState.Idle, PlaybackState.Loading, PlaybackState.Ended -> Unit
            is PlaybackState.Failed -> Unit
        }
    }

    fun seek(bySeconds: Double) {
        if (!_state.value.isSeekable) return
        _engine.value?.seekTo(bySeconds)
    }

    fun select(track: TrackId) {
        _engine.value?.select(track)
    }

    fun clearSubtitle() {
        _engine.value?.clearSubtitle()
    }

    fun cycleAspect() {
        val next = _state.value.aspect.next
        _state.update { it.copy(aspect = next) }
        _engine.value?.setAspect(next)
    }

    /**
     * Disposes the engine. The screen calls this from `DisposableEffect.onDispose`; `release` is
     * idempotent by contract, so it is safe alongside the terminal-state path.
     */
    fun release() {
        engineJob?.cancel()
        engineJob = null
        _engine.value?.release()
        _engine.value = null
    }

    override fun onCleared() {
        release()
    }

    // endregion

    // region Engine lifecycle

    private suspend fun play(
        target: PlayableChannel,
        engineOverride: EngineId?,
    ) {
        release()
        _state.update {
            it.copy(playback = PlaybackState.Loading, fallbackOffer = null, engineUnavailable = false)
        }

        val resolved = resolveEngine(target, engineOverride)
        val built = registry.make(resolved)
        if (built == null) {
            // Only reachable when the platform default itself is not registered — a wiring bug.
            // Report it as one honest failure rather than substituting an engine the policy did not
            // choose.
            Log.e(LOG_TAG, "no engine registered for ${resolved.value}")
            _state.update {
                it.copy(
                    playback = PlaybackState.Failed(EngineError.Unknown("engine ${resolved.value} not registered")),
                    engineUnavailable = true,
                )
            }
            return
        }

        _engine.value = built
        built.setAspect(_state.value.aspect)
        observe(built, resolved)
        loadStartedAt = TimeSource.Monotonic.markNow()
        Log.i(LOG_TAG, "load channel ${target.identity} on ${resolved.value}")
        built.load(request(target))
        built.play()
    }

    private suspend fun resolveEngine(
        target: PlayableChannel,
        override: EngineId?,
    ): EngineId {
        if (override != null) return override
        // Overrides are opaque strings in the core (engine identity is a shell concept,
        // TECH_SPEC §8); the mapping to `EngineId` happens here, where both layers are in scope.
        val channelKey = setting { access.channelEngine(target.sourceId, target.identity) }
        val sourceKey = setting { access.sourceEngine(target.sourceId) }
        return EngineSelection.resolve(
            channelOverride = channelKey?.let(::EngineId),
            sourceOverride = sourceKey?.let(::EngineId),
            platformDefault = registry.platformDefault,
            registered = registry.registered,
        )
    }

    private suspend fun request(target: PlayableChannel): StreamRequest {
        val profile =
            setting { access.bufferingProfile() }
                ?.let { key -> BufferingProfile.entries.firstOrNull { it.name.equals(key, ignoreCase = true) } }
                ?: BufferingProfile.BALANCED
        // The stored locator is not always playable: an Xtream catalog holds a credential-free one
        // so the password never reaches SQLite (TECH_SPEC §12), and this is where the credential
        // goes back. Resolved per play and never stored — the point of a credential-free catalog is
        // that the playable form does not outlive its use.
        //
        // Falling back to the stored locator is deliberate. For an M3U source the two are
        // identical, so the fallback is exact; for an Xtream source the engine then fails with its
        // own EngineError — the loud, actionable path (PRD §6.3) — instead of this returning a
        // request whose failure the viewer has no explanation for.
        val locator = setting { access.resolveStream(target.sourceId, target.locator) } ?: target.locator
        return StreamRequest(locator = locator, buffering = profile)
    }

    /**
     * Drains the engine's flows onto the state. One job per engine, cancelled on dispose, so a
     * zapped-away engine cannot write state for a channel the viewer already left.
     */
    private fun observe(
        built: PlaybackEngine,
        id: EngineId,
    ) {
        engineJob =
            viewModelScope.launch {
                launch { built.state.collect { apply(it, built, id) } }
                launch { built.tracks.collect { tracks -> ifCurrent(built) { copy(tracks = tracks) } } }
                launch { built.isSeekable.collect { seekable -> ifCurrent(built) { copy(isSeekable = seekable) } } }
            }
    }

    private fun apply(
        next: PlaybackState,
        built: PlaybackEngine,
        id: EngineId,
    ) {
        // A late event from a disposed engine (teardown races the event thread) must never move the
        // state of the channel now playing.
        if (_engine.value !== built) return
        _state.update { it.copy(playback = next) }

        if (next.isShowingVideo) {
            reportFirstFrame(id)
        }

        val error = next.failure
        if (error != null) {
            Log.e(LOG_TAG, "${id.value} failed: ${error.failureClass} ${error.diagnosticDetail.orEmpty()}")
            offerFallbackIfSensible(error, id)
        }
    }

    /**
     * The click-to-first-frame budget (PRD §9). Logged every time rather than sampled: the zap path
     * is profiled every release, and a budget you cannot see is a budget you do not keep.
     */
    private fun reportFirstFrame(id: EngineId) {
        val started = loadStartedAt ?: return
        loadStartedAt = null
        val elapsed = started.elapsedNow()
        Log.i(
            LOG_TAG,
            "first frame on ${id.value} in ${elapsed.inWholeMilliseconds} ms " +
                "(budget ${FIRST_FRAME_BUDGET.inWholeMilliseconds} ms)",
        )
        if (elapsed > FIRST_FRAME_BUDGET) {
            Log.w(LOG_TAG, "first frame exceeded budget on ${id.value}")
        }
    }

    /**
     * The loud-fallback rule (TECH_SPEC §8): offer another engine only when one could plausibly
     * succeed, and only when there is another engine to offer.
     */
    private fun offerFallbackIfSensible(
        error: EngineError,
        id: EngineId,
    ) {
        if (!error.offersOtherPlayer) return
        val alternate = EngineSelection.alternate(id, registry.registered) ?: return
        _state.update { it.copy(fallbackOffer = FallbackOffer(error = error, alternate = alternate)) }
    }

    private suspend fun rememberEngine(id: EngineId) {
        val channel = _state.value.channel
        try {
            access.setChannelEngine(channel.sourceId, channel.identity, id.value)
        } catch (e: CancellationException) {
            throw e
        } catch (e: ApiException) {
            Log.e(LOG_TAG, "remembering engine failed", e)
        }
    }

    private suspend fun loadWindow() {
        val loaded =
            try {
                access.zapWindow(context, offset)
            } catch (e: CancellationException) {
                throw e
            } catch (e: ApiException) {
                Log.w(LOG_TAG, "zap window failed", e)
                null
            }
        // A refresh can move offsets under a playing channel. Rather than zap somewhere the viewer
        // did not ask for, drop the ring and keep playing: the strip then shows no peek, which is
        // honest.
        val moved = loaded != null && loaded.current.identity != _state.value.channel.identity
        _state.update { it.copy(window = if (moved) null else loaded) }
    }

    private suspend fun recordRecent() {
        try {
            access.recordRecent(_state.value.channel)
        } catch (e: CancellationException) {
            throw e
        } catch (e: ApiException) {
            Log.w(LOG_TAG, "recording recent failed", e)
        }
    }

    /**
     * Reads an optional core setting, treating a failed read as "not set". A settings lookup that
     * fails must not stop a channel from playing; cancellation still propagates.
     */
    private suspend fun setting(read: suspend () -> String?): String? =
        try {
            read()
        } catch (e: CancellationException) {
            throw e
        } catch (e: ApiException) {
            Log.w(LOG_TAG, "reading playback setting failed", e)
            null
        }

    private inline fun ifCurrent(
        built: PlaybackEngine,
        change: PlaybackUiState.() -> PlaybackUiState,
    ) {
        if (_engine.value !== built) return
        _state.update(change)
    }

    // endregion

    companion object {
        fun factory(
            channel: PlayableChannel,
            context: ZapContext,
            offset: UInt,
            access: PlaybackAccess,
            registry: EngineRegistry,
        ): ViewModelProvider.Factory =
            viewModelFactory {
                initializer { PlaybackViewModel(channel, context, offset, access, registry) }
            }

        private const val LOG_TAG = "spidola::playback"

        /** The click-to-first-frame bar from PRD §9. */
        private val FIRST_FRAME_BUDGET = 2000.milliseconds
    }
}
