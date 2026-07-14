// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.core.playercontract

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update

/**
 * A scriptable in-memory engine for feature-code tests (TECH_SPEC §10: view models are unit-tested
 * against fakes, not real decoders).
 *
 * It ships in the module rather than a test source set because the playback slice's tests, the
 * app's Compose smoke suite, and any future engine's conformance checks all need the same fake —
 * three consumers is exactly when a shared unit is earned. It touches no media framework, so it is
 * the only engine that runs on the JVM without a device.
 *
 * Every state transition is driver-controlled: nothing here is timed, so a test that asserts the
 * loud-fallback path cannot flake on a decode that "usually" takes 40 ms.
 */
class FakeEngine(
    override val id: EngineId = IDENTITY,
) : PlaybackEngine {
    private val _state = MutableStateFlow<PlaybackState>(PlaybackState.Idle)
    override val state: StateFlow<PlaybackState> = _state.asStateFlow()

    private val _tracks = MutableStateFlow(TrackSelection())
    override val tracks: StateFlow<TrackSelection> = _tracks.asStateFlow()

    private val _isSeekable = MutableStateFlow(false)
    override val isSeekable: StateFlow<Boolean> = _isSeekable.asStateFlow()

    /** What [load] was called with — the zap path's assertion surface. */
    val loaded: List<StreamRequest> get() = _loaded.toList()
    private val _loaded = mutableListOf<StreamRequest>()

    var aspect: AspectMode = AspectMode.FIT
        private set

    var isReleased: Boolean = false
        private set

    var playCount: Int = 0
        private set

    var pauseCount: Int = 0
        private set

    val seeks: List<Double> get() = _seeks.toList()
    private val _seeks = mutableListOf<Double>()

    @Composable
    override fun Surface(modifier: Modifier) {
        Box(modifier.background(Color.Black))
    }

    override fun load(request: StreamRequest) {
        _loaded += request
        emit(PlaybackState.Loading)
    }

    override fun play() {
        playCount++
        emit(PlaybackState.Playing)
    }

    override fun pause() {
        pauseCount++
        emit(PlaybackState.Paused)
    }

    override fun seekTo(seconds: Double) {
        if (!_isSeekable.value) return
        _seeks += seconds
    }

    override fun select(track: TrackId) {
        val match = _tracks.value.available.firstOrNull { it.id == track } ?: return
        _tracks.update {
            when (match.kind) {
                TrackKind.AUDIO -> it.copy(selectedAudio = track)
                TrackKind.SUBTITLE -> it.copy(selectedSubtitle = track)
            }
        }
    }

    override fun clearSubtitle() {
        _tracks.update { it.copy(selectedSubtitle = null) }
    }

    override fun setAspect(mode: AspectMode) {
        aspect = mode
    }

    override fun release() {
        isReleased = true
    }

    // region Test driving

    /** Drives the engine to [state], as a real engine's event stream would. */
    fun simulate(state: PlaybackState) = emit(state)

    /** Publishes a track menu, as a real engine does once the stream's tracks are known. */
    fun simulateTracks(
        selection: TrackSelection,
        seekable: Boolean = false,
    ) {
        _tracks.value = selection
        _isSeekable.value = seekable
    }

    // endregion

    private fun emit(state: PlaybackState) {
        if (isReleased) return
        _state.value = state
    }

    companion object {
        /**
         * The engine identity a test resolves to. Distinct from any real engine's key, so a fake
         * can never satisfy a policy assertion that meant a real one.
         */
        val IDENTITY = EngineId("fake")
    }
}
