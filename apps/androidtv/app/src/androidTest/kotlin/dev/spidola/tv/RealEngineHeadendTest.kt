// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv

import android.os.SystemClock
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import dev.spidola.tv.core.playercontract.EngineError
import dev.spidola.tv.core.playercontract.PlaybackEngine
import dev.spidola.tv.core.playercontract.PlaybackState
import dev.spidola.tv.core.playercontract.StreamRequest
import dev.spidola.tv.player.engineexo.ExoEngine
import dev.spidola.tv.player.enginempv.MpvEngine
import org.junit.Assert.fail
import org.junit.Assume.assumeTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Opt-in device acceptance against the repository's deterministic headend.
 *
 * Normal instrumentation runs skip this suite. Maintainers enable it with
 * `-e spidolaHeadendBase http://10.0.2.2:8090` after starting `tools/test-headend`.
 * Each scenario creates a fresh engine because real engines are single-use by contract.
 */
@RunWith(AndroidJUnit4::class)
class RealEngineHeadendTest {
    @get:Rule
    val composeRule = createAndroidComposeRule<MainActivity>()

    private var renderedEngine by mutableStateOf<PlaybackEngine?>(null)

    @Before
    fun installEngineSurfaceHost() {
        composeRule.activity.setContent { EngineSurfaceHost() }
    }

    @Test
    fun exoPlayerReportsHeadendContract() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        assertEngine("ExoPlayer") { ExoEngine(context) }
    }

    @Test
    fun libmpvReportsHeadendContract() {
        // The Android TV emulator's MediaCodec surface path can stall independently of mpv.
        // Exercise the real libmpv decoder/render/event pipeline in software here; Phase 8's
        // physical-device matrix covers the default zero-copy MediaCodec path.
        assertEngine("libmpv") { MpvEngine(useHardwareDecoding = false) }
    }

    private fun assertEngine(
        name: String,
        factory: () -> PlaybackEngine,
    ) {
        val base = headendBase()
        assertScenario(name, factory, "$base/streams/hls-h264-aac/master.m3u8", Expected.Playing)
        assertScenario(name, factory, "$base/unreachable", Expected.Failed(EngineError.SourceUnreachable))
        assertScenario(name, factory, "$base/unauthorized", Expected.Failed(EngineError.Unauthorized))
        assertScenario(name, factory, "$base/unsupported-format", Expected.Failed(EngineError.UnsupportedFormat))
        assertScenario(name, factory, "$base/decoder-failed", Expected.Failed(EngineError.DecoderFailed))
        assertScenario(name, factory, "$base/timeout", Expected.Failed(EngineError.Timeout))
        assertScenario(name, factory, "$base/unknown", Expected.Unknown)
    }

    private fun assertScenario(
        engineName: String,
        factory: () -> PlaybackEngine,
        locator: String,
        expected: Expected,
    ) {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        lateinit var engine: PlaybackEngine
        composeRule.runOnIdle {
            engine = factory()
            renderedEngine = engine
        }
        composeRule.waitForIdle()
        instrumentation.runOnMainSync {
            engine.load(StreamRequest(locator))
        }

        try {
            val deadline = SystemClock.elapsedRealtime() + ROUTE_TIMEOUT_MS
            var playingSince: Long? = null
            while (SystemClock.elapsedRealtime() < deadline) {
                val observed = engine.state.value
                if (expected == Expected.Playing) {
                    if (observed == PlaybackState.Playing) {
                        val since = playingSince ?: SystemClock.elapsedRealtime().also { playingSince = it }
                        if (SystemClock.elapsedRealtime() - since >= PLAYING_STABILITY_MS) return
                    } else {
                        playingSince = null
                    }
                } else if (expected.matches(observed)) {
                    return
                }
                if (observed is PlaybackState.Failed || observed == PlaybackState.Ended) {
                    fail("$engineName reported $observed for $locator; expected $expected")
                }
                SystemClock.sleep(POLL_INTERVAL_MS)
            }
            fail("$engineName remained ${engine.state.value} for $locator; expected $expected")
        } finally {
            composeRule.runOnIdle {
                engine.release()
                renderedEngine = null
            }
            composeRule.waitForIdle()
        }
    }

    @androidx.compose.runtime.Composable
    private fun EngineSurfaceHost() {
        renderedEngine?.Surface(Modifier.fillMaxSize())
    }

    private fun headendBase(): String {
        val value =
            InstrumentationRegistry
                .getArguments()
                .getString(HEADEND_ARGUMENT)
                ?.trimEnd('/')
        assumeTrue("Set -e $HEADEND_ARGUMENT to run real-engine acceptance", !value.isNullOrBlank())
        return requireNotNull(value)
    }

    private sealed interface Expected {
        fun matches(state: PlaybackState): Boolean

        data object Playing : Expected {
            override fun matches(state: PlaybackState): Boolean = state == PlaybackState.Playing
        }

        data class Failed(
            val error: EngineError,
        ) : Expected {
            override fun matches(state: PlaybackState): Boolean = state is PlaybackState.Failed && state.error == error
        }

        data object Unknown : Expected {
            override fun matches(state: PlaybackState): Boolean = state is PlaybackState.Failed && state.error is EngineError.Unknown
        }
    }

    private companion object {
        const val HEADEND_ARGUMENT = "spidolaHeadendBase"
        const val ROUTE_TIMEOUT_MS = 75_000L
        const val POLL_INTERVAL_MS = 100L
        const val PLAYING_STABILITY_MS = 1_000L
    }
}
