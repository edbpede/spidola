// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.feature.playback

import dev.spidola.tv.core.corekit.PlayableChannel
import dev.spidola.tv.core.corekit.PlaybackAccess
import dev.spidola.tv.core.corekit.ZapContext
import dev.spidola.tv.core.corekit.ZapWindow
import dev.spidola.tv.core.playercontract.EngineError
import dev.spidola.tv.core.playercontract.EngineId
import dev.spidola.tv.core.playercontract.EngineRegistry
import dev.spidola.tv.core.playercontract.FakeEngine
import dev.spidola.tv.core.playercontract.PlaybackEngine
import dev.spidola.tv.core.playercontract.PlaybackState
import dev.spidola.tv.core.playercontract.failure
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.jupiter.api.AfterEach
import org.junit.jupiter.api.BeforeEach
import org.junit.jupiter.api.Test
import uniffi.core_api.MediaKind
import kotlin.test.assertEquals
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertTrue

/**
 * The playback slice's view-model logic against a fake corekit and the contract's `FakeEngine`
 * (TECH_SPEC §10) — no decoder, no network, no timing.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class PlaybackViewModelTest {
    private val dispatcher = StandardTestDispatcher()

    @BeforeEach
    fun setUp() {
        Dispatchers.setMain(dispatcher)
    }

    @AfterEach
    fun tearDown() {
        Dispatchers.resetMain()
    }

    // region Engine selection

    @Test
    fun `uses the platform default when there are no overrides`() =
        runTest(dispatcher) {
            val harness = Harness()
            harness.viewModel().start()
            advanceUntilIdle()
            assertEquals(listOf("exoplayer"), harness.built.map { it.value })
        }

    @Test
    fun `honours a channel override over a source override`() =
        runTest(dispatcher) {
            val harness = Harness()
            harness.access.channelEngines["1-10"] = "mpv"
            harness.access.sourceEngines[1] = "exoplayer"
            harness.viewModel().start()
            advanceUntilIdle()
            assertEquals(listOf("mpv"), harness.built.map { it.value })
        }

    @Test
    fun `honours a source override when there is no channel override`() =
        runTest(dispatcher) {
            val harness = Harness()
            harness.access.sourceEngines[1] = "mpv"
            harness.viewModel().start()
            advanceUntilIdle()
            assertEquals(listOf("mpv"), harness.built.map { it.value })
        }

    /** A stale key from another platform must not make a channel unplayable. */
    @Test
    fun `a stale override falls back to the default`() =
        runTest(dispatcher) {
            val harness = Harness()
            harness.access.channelEngines["1-10"] = "avplayer"
            harness.viewModel().start()
            advanceUntilIdle()
            assertEquals(listOf("exoplayer"), harness.built.map { it.value })
        }

    /** A composition bug must surface as one honest failure, not a blank screen. */
    @Test
    fun `an unregistered default reports the engine unavailable`() =
        runTest(dispatcher) {
            val harness = Harness(registered = emptySet())
            val viewModel = harness.viewModel()
            viewModel.start()
            advanceUntilIdle()
            assertTrue(viewModel.state.value.engineUnavailable)
            assertNotNull(viewModel.state.value.playback.failure)
        }

    // endregion

    // region Loud fallback

    @Test
    fun `a format failure offers the other player`() =
        runTest(dispatcher) {
            val harness = Harness()
            val viewModel = harness.viewModel()
            viewModel.start()
            advanceUntilIdle()
            harness.engines[0].simulate(PlaybackState.Failed(EngineError.UnsupportedFormat))
            advanceUntilIdle()
            assertEquals(EngineId.MPV, viewModel.state.value.fallbackOffer?.alternate)
        }

    @Test
    fun `a decoder failure offers the other player`() =
        runTest(dispatcher) {
            val harness = Harness()
            val viewModel = harness.viewModel()
            viewModel.start()
            advanceUntilIdle()
            harness.engines[0].simulate(PlaybackState.Failed(EngineError.DecoderFailed))
            advanceUntilIdle()
            assertEquals(EngineId.MPV, viewModel.state.value.fallbackOffer?.alternate)
        }

    /** A network failure would fail identically on any engine — offering a swap would be a lie. */
    @Test
    fun `a network failure does not offer the other player`() =
        runTest(dispatcher) {
            val harness = Harness()
            val viewModel = harness.viewModel()
            viewModel.start()
            advanceUntilIdle()
            harness.engines[0].simulate(PlaybackState.Failed(EngineError.SourceUnreachable))
            advanceUntilIdle()
            assertNull(viewModel.state.value.fallbackOffer)
            assertEquals(EngineError.SourceUnreachable, viewModel.state.value.playback.failure)
        }

    @Test
    fun `an unauthorized failure does not offer the other player`() =
        runTest(dispatcher) {
            val harness = Harness()
            val viewModel = harness.viewModel()
            viewModel.start()
            advanceUntilIdle()
            harness.engines[0].simulate(PlaybackState.Failed(EngineError.Unauthorized))
            advanceUntilIdle()
            assertNull(viewModel.state.value.fallbackOffer)
        }

    /** With nothing else registered there is nothing honest to offer. */
    @Test
    fun `no offer when only one engine is registered`() =
        runTest(dispatcher) {
            val harness = Harness(registered = setOf(EngineId.EXOPLAYER))
            val viewModel = harness.viewModel()
            viewModel.start()
            advanceUntilIdle()
            harness.engines[0].simulate(PlaybackState.Failed(EngineError.UnsupportedFormat))
            advanceUntilIdle()
            assertNull(viewModel.state.value.fallbackOffer)
        }

    // endregion

    // region Try other player

    @Test
    fun `try other player rebuilds on the alternate and remembers`() =
        runTest(dispatcher) {
            val harness = Harness()
            val viewModel = harness.viewModel()
            viewModel.start()
            advanceUntilIdle()
            harness.engines[0].simulate(PlaybackState.Failed(EngineError.UnsupportedFormat))
            advanceUntilIdle()
            viewModel.tryOtherPlayer(remember = true)
            advanceUntilIdle()
            assertEquals(listOf("exoplayer", "mpv"), harness.built.map { it.value })
            assertEquals("mpv", harness.access.channelEngines["1-10"])
            assertNull(viewModel.state.value.fallbackOffer)
        }

    /** "Just this once" must not write a preference. */
    @Test
    fun `try other player without remember does not persist`() =
        runTest(dispatcher) {
            val harness = Harness()
            val viewModel = harness.viewModel()
            viewModel.start()
            advanceUntilIdle()
            harness.engines[0].simulate(PlaybackState.Failed(EngineError.UnsupportedFormat))
            advanceUntilIdle()
            viewModel.tryOtherPlayer(remember = false)
            advanceUntilIdle()
            assertEquals(listOf("exoplayer", "mpv"), harness.built.map { it.value })
            assertNull(harness.access.channelEngines["1-10"])
        }

    /** The previous engine must be torn down — a leaked decoder per fallback would be fatal on TV. */
    @Test
    fun `try other player disposes the failed engine`() =
        runTest(dispatcher) {
            val harness = Harness()
            val viewModel = harness.viewModel()
            viewModel.start()
            advanceUntilIdle()
            harness.engines[0].simulate(PlaybackState.Failed(EngineError.UnsupportedFormat))
            advanceUntilIdle()
            viewModel.tryOtherPlayer(remember = false)
            advanceUntilIdle()
            assertTrue(harness.engines[0].isReleased)
        }

    // endregion

    // region Zap

    @Test
    fun `zap next loads the following channel and disposes the previous engine`() =
        runTest(dispatcher) {
            val harness = Harness()
            val viewModel = harness.viewModel()
            viewModel.start()
            advanceUntilIdle()
            viewModel.zap(ZapDirection.NEXT)
            advanceUntilIdle()
            assertEquals(11L, viewModel.state.value.channel.identity)
            assertTrue(harness.engines[0].isReleased)
            assertEquals(2, harness.built.size)
        }

    @Test
    fun `zap previous at the start is a no-op`() =
        runTest(dispatcher) {
            val harness = Harness()
            val viewModel = harness.viewModel()
            viewModel.start()
            advanceUntilIdle()
            viewModel.zap(ZapDirection.PREVIOUS)
            advanceUntilIdle()
            assertEquals(10L, viewModel.state.value.channel.identity)
            assertEquals(1, harness.built.size)
        }

    /**
     * A refresh can move offsets under a playing channel; the ring is dropped rather than zapping
     * somewhere the viewer did not ask for.
     */
    @Test
    fun `the window is dropped when the ring moved under the channel`() =
        runTest(dispatcher) {
            val harness = Harness()
            harness.access.windowIdentityOverride = 999
            val viewModel = harness.viewModel()
            viewModel.start()
            advanceUntilIdle()
            assertNull(viewModel.state.value.window)
            viewModel.zap(ZapDirection.NEXT)
            advanceUntilIdle()
            assertEquals(10L, viewModel.state.value.channel.identity)
        }

    // endregion

    // region Transport

    @Test
    fun `release disposes the engine`() =
        runTest(dispatcher) {
            val harness = Harness()
            val viewModel = harness.viewModel()
            viewModel.start()
            advanceUntilIdle()
            viewModel.release()
            assertTrue(harness.engines[0].isReleased)
            assertNull(viewModel.engine.value)
        }

    @Test
    fun `records a recent on play`() =
        runTest(dispatcher) {
            val harness = Harness()
            harness.viewModel().start()
            advanceUntilIdle()
            assertEquals(listOf(10L), harness.access.recorded.map { it.identity })
        }

    // endregion
}

// region Harness

private class Harness(
    private val registered: Set<EngineId> = setOf(EngineId.EXOPLAYER, EngineId.MPV),
) {
    val access = FakePlaybackAccess()
    val built = mutableListOf<EngineId>()
    val engines = mutableListOf<FakeEngine>()

    fun viewModel(): PlaybackViewModel =
        PlaybackViewModel(
            channel = channel(identity = 10, name = "BBC One"),
            context = ZapContext.Group(sourceId = 1, kind = MediaKind.LIVE, group = "News"),
            offset = 0u,
            access = access,
            registry = registry(),
        )

    private fun registry(): EngineRegistry {
        val factories: Map<EngineId, () -> PlaybackEngine> = registered.associateWith { id -> { build(id) } }
        return EngineRegistry(platformDefault = EngineId.EXOPLAYER, factories = factories)
    }

    private fun build(id: EngineId): PlaybackEngine {
        val engine = FakeEngine(id)
        built += id
        engines += engine
        return engine
    }

    companion object {
        fun channel(
            identity: Long,
            name: String,
        ): PlayableChannel =
            PlayableChannel(
                sourceId = 1,
                identity = identity,
                name = name,
                group = "News",
                logo = null,
                locator = "http://host.example/$identity.ts",
                kind = MediaKind.LIVE,
            )
    }
}

private class FakePlaybackAccess : PlaybackAccess {
    val channelEngines = mutableMapOf<String, String>()
    val sourceEngines = mutableMapOf<Long, String>()
    val recorded = mutableListOf<PlayableChannel>()
    var buffering: String? = null

    /** Forces the window's current row to a different identity, as a refresh would. */
    var windowIdentityOverride: Long? = null

    override suspend fun zapWindow(
        context: ZapContext,
        offset: UInt,
    ): ZapWindow {
        val identity = windowIdentityOverride ?: (10L + offset.toLong())
        return ZapWindow(
            previous = if (offset == 0u) null else Harness.channel(9L + offset.toLong(), "Prev"),
            current = Harness.channel(identity, "Current"),
            next = Harness.channel(11L + offset.toLong(), "Next"),
            offset = offset,
            total = 24uL,
        )
    }

    override suspend fun channelEngine(
        sourceId: Long,
        identity: Long,
    ): String? = channelEngines["$sourceId-$identity"]

    override suspend fun setChannelEngine(
        sourceId: Long,
        identity: Long,
        engine: String?,
    ) {
        val key = "$sourceId-$identity"
        if (engine == null) channelEngines.remove(key) else channelEngines[key] = engine
    }

    override suspend fun sourceEngine(sourceId: Long): String? = sourceEngines[sourceId]

    override suspend fun bufferingProfile(): String? = buffering

    override suspend fun setBufferingProfile(profile: String) {
        buffering = profile
    }

    override suspend fun recordRecent(channel: PlayableChannel) {
        recorded += channel
    }
}

// endregion
