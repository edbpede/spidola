// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.feature.browse

import dev.spidola.tv.core.corekit.CatalogAccess
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
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
import uniffi.core_api.ApiException
import uniffi.core_api.Channel
import uniffi.core_api.ChannelOverrides
import uniffi.core_api.ChannelPage
import uniffi.core_api.MediaKind
import uniffi.core_api.Source
import uniffi.core_api.SourceCommon

@OptIn(ExperimentalCoroutinesApi::class)
class BrowseViewModelTest {
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
    fun `empty when there are no sources`() =
        runTest(dispatcher) {
            val viewModel = BrowseViewModel(FakeCatalog(sourceList = emptyList()))
            advanceUntilIdle()
            assertEquals(BrowseUiState.Empty, viewModel.state.value)
        }

    @Test
    fun `empty when the source has no channels`() =
        runTest(dispatcher) {
            val viewModel =
                BrowseViewModel(
                    FakeCatalog(sourceList = listOf(source(1L)), pages = mapOf(1L to channelPage(emptyList()))),
                )
            advanceUntilIdle()
            assertEquals(BrowseUiState.Empty, viewModel.state.value)
        }

    @Test
    fun `ready lists the source's channels in order`() =
        runTest(dispatcher) {
            val viewModel =
                BrowseViewModel(
                    FakeCatalog(
                        sourceList = listOf(source(1L)),
                        pages = mapOf(1L to channelPage(listOf(channel(1L, "News"), channel(2L, "Sports", "Live")))),
                    ),
                )
            advanceUntilIdle()
            val state = viewModel.state.value
            check(state is BrowseUiState.Ready)
            assertEquals(listOf("News", "Sports"), state.channels.map { it.name })
            assertEquals(listOf(IDENTITY_STRIDE, 2L * IDENTITY_STRIDE), state.channels.map { it.key })
        }

    @Test
    fun `error state when the catalog query fails`() =
        runTest(dispatcher) {
            val viewModel =
                BrowseViewModel(
                    FakeCatalog(sourceList = listOf(source(1L)), failWith = ApiException.StorageCorrupt()),
                )
            advanceUntilIdle()
            assertTrue(viewModel.state.value is BrowseUiState.Error)
        }

    private companion object {
        const val IDENTITY_STRIDE = 10L
    }
}

private class FakeCatalog(
    private val sourceList: List<Source>,
    private val pages: Map<Long, ChannelPage> = emptyMap(),
    private val failWith: ApiException? = null,
) : CatalogAccess {
    override suspend fun sources(): List<Source> = sourceList

    override suspend fun page(
        sourceId: Long,
        offset: UInt,
        limit: UInt,
    ): ChannelPage {
        failWith?.let { throw it }
        return pages[sourceId] ?: channelPage(emptyList())
    }
}

private fun source(id: Long): Source =
    Source.M3uUrl(
        id = id,
        common = SourceCommon(name = "Fixture", enabled = true, autoRefreshSecs = null),
        url = "http://host.example/list.m3u",
        userAgent = null,
        acceptInvalidTls = false,
    )

private fun channel(
    id: Long,
    name: String,
    group: String? = null,
): Channel =
    Channel(
        id = id,
        sourceId = 1L,
        identity = id * 10L,
        name = name,
        groupTitle = group,
        logo = null,
        locator = "http://host.example/$id.ts",
        kind = MediaKind.LIVE,
        categoryId = null,
        overrides = ChannelOverrides(userAgent = null, headers = emptyList(), preferredEngine = null),
    )

private fun channelPage(channels: List<Channel>): ChannelPage =
    ChannelPage(channels = channels, offset = 0u, total = channels.size.toULong())
