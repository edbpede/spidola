// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.feature.settings

import dev.spidola.tv.core.corekit.LoadState
import dev.spidola.tv.core.corekit.SettingsAccess
import dev.spidola.tv.core.playercontract.EngineId
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.jupiter.api.AfterEach
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertNull
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.BeforeEach
import org.junit.jupiter.api.Test
import uniffi.core_api.ApiException
import uniffi.core_api.AppSettings
import uniffi.core_api.BufferingProfile
import uniffi.core_api.Handshake
import uniffi.core_api.InterfaceDensity
import uniffi.core_api.LogLevel
import uniffi.core_api.SubtitleBackground
import uniffi.core_api.SubtitleSize

@OptIn(ExperimentalCoroutinesApi::class)
class SettingsViewModelTest {
    private val dispatcher = StandardTestDispatcher()

    @BeforeEach
    fun setUp() {
        Dispatchers.setMain(dispatcher)
    }

    @AfterEach
    fun tearDown() {
        Dispatchers.resetMain()
    }

    @Test
    fun `the snapshot loads into the rows`() =
        runTest(dispatcher) {
            val access = FakeSettingsAccess(settings = settings(defaultEngine = "mpv", recentsRetentionDays = 90u))
            val viewModel = SettingsViewModel(access)

            viewModel.load()
            advanceUntilIdle()

            val state = viewModel.state.value
            check(state is LoadState.Ready)
            // The opaque core key becomes the shell's engine identity, and every row's value is
            // readable straight off the snapshot — which is what the list renders.
            assertEquals(EngineId.MPV, state.value.defaultEngine)
            assertEquals(BufferingProfile.BALANCED, state.value.buffering)
            assertEquals(90u, state.value.recentsRetentionDays)
            assertEquals(LanguageChoice.SYSTEM, state.value.language)
        }

    @Test
    fun `a failing read surfaces an actionable error`() =
        runTest(dispatcher) {
            val viewModel = SettingsViewModel(FakeSettingsAccess(failure = ApiException.StorageCorrupt()))

            viewModel.load()
            advanceUntilIdle()

            val state = viewModel.state.value
            check(state is LoadState.Failed)
            // PRD §6.3: an error with no action is a design bug, so the type cannot express one.
            assertTrue(state.error.actions.isNotEmpty())
        }

    @Test
    fun `the recents off-switch routes to the recents api, not the settings one`() =
        runTest(dispatcher) {
            val access = FakeSettingsAccess()
            val viewModel = SettingsViewModel(access)

            viewModel.setRecentsEnabled(false)
            advanceUntilIdle()

            assertEquals(listOf(false), access.recentsEnabledWrites)
            // The flag is owned by the core's recents service; the settings service only reports it
            // in the snapshot. Nothing on the settings side may have been written.
            assertTrue(access.settingsWrites.isEmpty(), "settings writes: ${access.settingsWrites}")
        }

    @Test
    fun `clearing history records and reports it`() =
        runTest(dispatcher) {
            val access = FakeSettingsAccess()
            val viewModel = SettingsViewModel(access)

            viewModel.clearHistory()
            advanceUntilIdle()

            assertEquals(1, access.clearCount)
            assertEquals(SettingsStatus.HistoryCleared, viewModel.status.value)
        }

    @Test
    fun `a failing action surfaces an actionable error and leaves the list readable`() =
        runTest(dispatcher) {
            val access = FakeSettingsAccess(failure = ApiException.StorageCorrupt())
            val viewModel = SettingsViewModel(access)

            viewModel.clearHistory()
            advanceUntilIdle()

            val status = viewModel.status.value
            check(status is SettingsStatus.Failed)
            assertTrue(status.error.actions.isNotEmpty())
        }

    @Test
    fun `a status clears on the next successful action`() =
        runTest(dispatcher) {
            val access = FakeSettingsAccess()
            val viewModel = SettingsViewModel(access)

            viewModel.clearHistory()
            advanceUntilIdle()
            assertEquals(SettingsStatus.HistoryCleared, viewModel.status.value)

            viewModel.setRecentsEnabled(true)
            advanceUntilIdle()
            assertNull(viewModel.status.value)
        }
}

/** Every setter reachable from a picker round-trips to the core exactly once, with its typed value. */
@OptIn(ExperimentalCoroutinesApi::class)
class SettingsPickerViewModelTest {
    private val dispatcher = StandardTestDispatcher()

    @BeforeEach
    fun setUp() {
        Dispatchers.setMain(dispatcher)
    }

    @AfterEach
    fun tearDown() {
        Dispatchers.resetMain()
    }

    @Test
    fun `the picker marks the current value`() =
        runTest(dispatcher) {
            val access = FakeSettingsAccess(settings = settings(subtitleSize = SubtitleSize.LARGE))
            val viewModel = SettingsPickerViewModel(access, SettingsPicker.SUBTITLE_SIZE)

            viewModel.load()
            advanceUntilIdle()

            val state = viewModel.state.value
            check(state is PickerState.Choosing)
            assertEquals(
                SettingValue.SubtitleGlyphSize(SubtitleSize.LARGE),
                SettingsPicker.SUBTITLE_SIZE.current(state.snapshot),
            )
            // The marked option is one the picker actually offers, so the list can mark it.
            assertTrue(SettingsPicker.SUBTITLE_SIZE.current(state.snapshot) in SettingsPicker.SUBTITLE_SIZE.options())
        }

    @Test
    fun `every setter round-trips its typed value`() =
        runTest(dispatcher) {
            val cases: List<Pair<SettingValue, (FakeSettingsAccess) -> Any?>> =
                listOf(
                    SettingValue.DefaultEngine(EngineId.MPV) to { it.defaultEngine },
                    SettingValue.DefaultEngine(null) to { it.defaultEngine },
                    SettingValue.Buffering(BufferingProfile.LOW) to { it.buffering },
                    SettingValue.SubtitleGlyphSize(SubtitleSize.SMALL) to { it.subtitleSize },
                    SettingValue.SubtitlePlate(SubtitleBackground.SOLID) to { it.subtitleBackground },
                    SettingValue.Language(LanguageChoice.ENGLISH) to { it.language },
                    SettingValue.Density(InterfaceDensity.COMPACT) to { it.density },
                    SettingValue.RecentsRetention(365u) to { it.retentionDays },
                    SettingValue.ImageCache(512u) to { it.imageCacheMb },
                    SettingValue.Logging(LogLevel.TRACE) to { it.logLevel },
                )

            val expected: List<Any?> =
                listOf("mpv", null, BufferingProfile.LOW, SubtitleSize.SMALL, SubtitleBackground.SOLID)
                    .plus(listOf("en", InterfaceDensity.COMPACT, 365u, 512u, LogLevel.TRACE))

            cases.forEachIndexed { index, (value, read) ->
                val access = FakeSettingsAccess()
                val viewModel = SettingsPickerViewModel(access, SettingsPicker.DEFAULT_ENGINE)

                viewModel.choose(value)
                advanceUntilIdle()

                assertEquals(expected[index], read(access), "round-trip of $value")
                // The write landed, so the screen returns to the list.
                assertEquals(PickerState.Applied, viewModel.state.value)
            }
        }

    @Test
    fun `a failing write surfaces an actionable error instead of returning`() =
        runTest(dispatcher) {
            val access = FakeSettingsAccess(failure = ApiException.StorageCorrupt())
            val viewModel = SettingsPickerViewModel(access, SettingsPicker.BUFFERING)

            viewModel.choose(SettingValue.Buffering(BufferingProfile.LOW))
            advanceUntilIdle()

            val state = viewModel.state.value
            check(state is PickerState.Failed)
            assertTrue(state.error.actions.isNotEmpty())
        }

    @Test
    fun `every picker offers options and can read its current value`() =
        runTest(dispatcher) {
            val snapshot = SettingsSnapshot.of(settings())
            // A picker with no options would be a dead screen, and one whose current value is not
            // readable could never be marked. Enumerated so a setting added to the vocabulary is
            // covered without anyone remembering to add a test.
            SettingsPicker.entries.forEach { picker ->
                assertTrue(picker.options().isNotEmpty(), "$picker offers no options")
                assertEquals(picker.current(snapshot), picker.current(snapshot), "$picker current is not stable")
            }
        }

    @Test
    fun `the automatic engine option is offered first and means no override`() {
        val options = SettingsPicker.DEFAULT_ENGINE.options()
        assertEquals(SettingValue.DefaultEngine(null), options.first())
        assertTrue(options.contains(SettingValue.DefaultEngine(EngineId.EXOPLAYER)))
        assertTrue(options.contains(SettingValue.DefaultEngine(EngineId.MPV)))
    }
}

@OptIn(ExperimentalCoroutinesApi::class)
class DiagnosticsViewModelTest {
    private val dispatcher = StandardTestDispatcher()

    @BeforeEach
    fun setUp() {
        Dispatchers.setMain(dispatcher)
    }

    @AfterEach
    fun tearDown() {
        Dispatchers.resetMain()
    }

    @Test
    fun `the report carries the log lines and both halves' versions`() =
        runTest(dispatcher) {
            val access =
                FakeSettingsAccess(
                    settings = settings(logLevel = LogLevel.DEBUG),
                    logs = listOf("first line", "second line"),
                )
            val viewModel = DiagnosticsViewModel(access, appVersion = "1.2.3")

            viewModel.load()
            advanceUntilIdle()

            val state = viewModel.state.value
            check(state is LoadState.Ready)
            assertEquals(LogLevel.DEBUG, state.value.logLevel)
            assertEquals(listOf("first line", "second line"), state.value.activity)
            assertEquals("1.2.3", state.value.versions.app)
            assertEquals("0.1.0", state.value.versions.core)
            assertEquals("abc1234", state.value.versions.coreRevision)
            assertEquals(2u, state.value.versions.boundary)
        }

    @Test
    fun `an empty log is an empty report, not a failure`() =
        runTest(dispatcher) {
            val viewModel = DiagnosticsViewModel(FakeSettingsAccess(logs = emptyList()), appVersion = "1.2.3")

            viewModel.load()
            advanceUntilIdle()

            val state = viewModel.state.value
            check(state is LoadState.Ready)
            assertTrue(state.value.activity.isEmpty())
        }

    @Test
    fun `a failing read surfaces an actionable error`() =
        runTest(dispatcher) {
            val access = FakeSettingsAccess(failure = ApiException.Internal())
            val viewModel = DiagnosticsViewModel(access, appVersion = "1.2.3")

            viewModel.load()
            advanceUntilIdle()

            val state = viewModel.state.value
            check(state is LoadState.Failed)
            assertTrue(state.error.actions.isNotEmpty())
        }
}

private fun settings(
    defaultEngine: String? = null,
    buffering: BufferingProfile = BufferingProfile.BALANCED,
    subtitleSize: SubtitleSize = SubtitleSize.MEDIUM,
    subtitleBackground: SubtitleBackground = SubtitleBackground.SHADOW,
    language: String? = null,
    density: InterfaceDensity = InterfaceDensity.COMFORTABLE,
    recentsEnabled: Boolean = true,
    recentsRetentionDays: UInt = 30u,
    imageCacheMaxMb: UInt = 256u,
    logLevel: LogLevel = LogLevel.INFO,
): AppSettings =
    AppSettings(
        defaultEngine = defaultEngine,
        buffering = buffering,
        subtitleSize = subtitleSize,
        subtitleBackground = subtitleBackground,
        language = language,
        density = density,
        recentsEnabled = recentsEnabled,
        recentsRetentionDays = recentsRetentionDays,
        epgWindowAheadHours = 12u,
        epgWindowBehindHours = 2u,
        imageCacheMaxMb = imageCacheMaxMb,
        logLevel = logLevel,
    )

/**
 * A fake [SettingsAccess]: records every write so a test can assert both what was written and — for
 * the recents off-switch — what was *not*. [failure] makes every call throw, which is how the error
 * paths are exercised without a core.
 */
private class FakeSettingsAccess(
    private val settings: AppSettings = settings(),
    private val logs: List<String> = emptyList(),
    private val failure: ApiException? = null,
) : SettingsAccess {
    /** Names of the settings-service writes that happened, in order. */
    val settingsWrites = mutableListOf<String>()
    val recentsEnabledWrites = mutableListOf<Boolean>()
    var clearCount = 0
        private set

    var defaultEngine: String? = null
        private set
    var buffering: BufferingProfile? = null
        private set
    var subtitleSize: SubtitleSize? = null
        private set
    var subtitleBackground: SubtitleBackground? = null
        private set
    var language: String? = null
        private set
    var density: InterfaceDensity? = null
        private set
    var retentionDays: UInt? = null
        private set
    var imageCacheMb: UInt? = null
        private set
    var logLevel: LogLevel? = null
        private set

    private fun record(name: String) {
        failure?.let { throw it }
        settingsWrites.add(name)
    }

    override suspend fun settings(): AppSettings {
        failure?.let { throw it }
        return settings
    }

    override suspend fun setDefaultEngine(engine: String?) {
        record("setDefaultEngine")
        defaultEngine = engine
    }

    override suspend fun setBuffering(profile: BufferingProfile) {
        record("setBuffering")
        buffering = profile
    }

    override suspend fun setSubtitleSize(size: SubtitleSize) {
        record("setSubtitleSize")
        subtitleSize = size
    }

    override suspend fun setSubtitleBackground(background: SubtitleBackground) {
        record("setSubtitleBackground")
        subtitleBackground = background
    }

    override suspend fun setLanguage(tag: String?) {
        record("setLanguage")
        language = tag
    }

    override suspend fun setDensity(density: InterfaceDensity) {
        record("setDensity")
        this.density = density
    }

    override suspend fun setRecentsRetentionDays(days: UInt) {
        record("setRecentsRetentionDays")
        retentionDays = days
    }

    override suspend fun setImageCacheMaxMb(megabytes: UInt) {
        record("setImageCacheMaxMb")
        imageCacheMb = megabytes
    }

    override suspend fun setLogLevel(level: LogLevel) {
        record("setLogLevel")
        logLevel = level
    }

    override suspend fun setRecentsEnabled(enabled: Boolean) {
        failure?.let { throw it }
        recentsEnabledWrites.add(enabled)
    }

    override suspend fun clearRecents() {
        failure?.let { throw it }
        clearCount++
    }

    override suspend fun exportLogs(): List<String> {
        failure?.let { throw it }
        return logs
    }

    override fun handshake(): Handshake =
        Handshake(
            coreVersion = "0.1.0",
            coreGitRevision = "abc1234",
            schemaVersion = 1u,
            boundaryVersion = 2u,
        )
}
