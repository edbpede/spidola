// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.feature.settings

import dev.spidola.tv.core.corekit.EpgAccess
import dev.spidola.tv.core.corekit.EpgRefreshEvent
import dev.spidola.tv.core.corekit.EpgWindowSettings
import dev.spidola.tv.core.corekit.LoadState
import dev.spidola.tv.core.corekit.name
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.jupiter.api.AfterEach
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.BeforeEach
import org.junit.jupiter.api.Test
import uniffi.core_api.ChannelNowNext
import uniffi.core_api.EpgPage
import uniffi.core_api.NowNext
import uniffi.core_api.Source
import uniffi.core_api.SourceCommon

@OptIn(ExperimentalCoroutinesApi::class)
class GuideViewModelTest {
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
    fun `guide loads sources and their configured state`() =
        runTest(dispatcher) {
            val viewModel = GuideViewModel(FakeGuideAccess())
            advanceUntilIdle()

            val state = viewModel.state.value
            check(state is LoadState.Ready)
            assertEquals("Fixture", state.value.sources.single().source.name)
            assertTrue(state.value.sources.single().hasFeed)
        }

    @Test
    fun `saving a feed delegates the trimmed address and reloads`() =
        runTest(dispatcher) {
            val access = FakeGuideAccess()
            val viewModel = GuideViewModel(access)
            advanceUntilIdle()

            viewModel.setFeed(1L, "  https://guide.example/epg.xml  ")
            advanceUntilIdle()

            assertEquals("https://guide.example/epg.xml", access.savedUrl)
        }
}

private class FakeGuideAccess : EpgAccess {
    var savedUrl: String? = null

    override suspend fun guideSources(): List<Source> =
        listOf(
            Source.M3uUrl(
                id = 1L,
                common = SourceCommon("Fixture", enabled = true, autoRefreshSecs = null),
                hasUserAgent = false,
                acceptInvalidTls = false,
            ),
        )

    override suspend fun epgWindowSettings(): EpgWindowSettings = EpgWindowSettings(12u, 2u)

    override suspend fun setEpgWindow(
        aheadHours: UInt,
        behindHours: UInt,
    ) = Unit

    override suspend fun nowNext(
        sourceId: Long,
        channelIdentity: Long,
        nowUnix: Long,
    ): NowNext = NowNext(null, null)

    override suspend fun nowNextBatch(
        sourceId: Long,
        channelIdentities: List<Long>,
        nowUnix: Long,
    ): List<ChannelNowNext> = channelIdentities.map { ChannelNowNext(it, NowNext(null, null)) }

    override suspend fun epgWindow(
        sourceId: Long,
        channelIdentity: Long,
        earliestUnix: Long,
        latestUnix: Long,
        offset: UInt,
        limit: UInt,
    ): EpgPage = EpgPage(emptyList(), offset)

    override suspend fun hasEpgFeed(sourceId: Long): Boolean = true

    override suspend fun setXmltvFeed(
        sourceId: Long,
        url: String,
    ) {
        savedUrl = url
    }

    override suspend fun clearXmltvFeed(sourceId: Long) = Unit

    override fun refreshEpg(
        sourceId: Long,
        nowUnix: Long,
    ): Flow<EpgRefreshEvent> = emptyFlow()
}
