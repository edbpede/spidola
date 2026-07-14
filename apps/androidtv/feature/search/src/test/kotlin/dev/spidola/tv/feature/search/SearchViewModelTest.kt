// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.feature.search

import dev.spidola.tv.core.corekit.SearchAccess
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
import uniffi.core_api.MediaKind
import uniffi.core_api.SearchPage
import uniffi.core_api.Source

@OptIn(ExperimentalCoroutinesApi::class)
class SearchViewModelTest {
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
    fun `blank query is idle`() =
        runTest(dispatcher) {
            val viewModel = SearchViewModel(FakeSearchAccess())
            viewModel.search("   ", null, null)
            advanceUntilIdle()
            assertEquals(SearchState.Idle, viewModel.state.value)
        }

    @Test
    fun `query produces results`() =
        runTest(dispatcher) {
            val viewModel = SearchViewModel(FakeSearchAccess(results = listOf(channel(1L, "BBC News"))))
            viewModel.search("bbc", null, null)
            advanceUntilIdle()
            val state = viewModel.state.value
            check(state is SearchState.Results)
            assertEquals(listOf("BBC News"), state.results.channels.map { it.name })
        }

    @Test
    fun `no matches is empty`() =
        runTest(dispatcher) {
            val viewModel = SearchViewModel(FakeSearchAccess(results = emptyList()))
            viewModel.search("zzz", null, null)
            advanceUntilIdle()
            assertEquals(SearchState.Empty, viewModel.state.value)
        }

    @Test
    fun `failure surfaces an actionable error`() =
        runTest(dispatcher) {
            val viewModel = SearchViewModel(FakeSearchAccess(failWith = ApiException.StorageCorrupt()))
            viewModel.search("x", null, null)
            advanceUntilIdle()
            val state = viewModel.state.value
            check(state is SearchState.Failed)
            assertTrue(state.error.actions.isNotEmpty())
        }

    private fun channel(
        identity: Long,
        name: String,
    ): Channel =
        Channel(
            id = identity,
            sourceId = 1L,
            identity = identity,
            name = name,
            groupTitle = null,
            logo = null,
            locator = "http://host.example/$identity.ts",
            kind = MediaKind.LIVE,
            categoryId = null,
            overrides = ChannelOverrides(userAgent = null, headers = emptyList(), preferredEngine = null),
        )
}

private class FakeSearchAccess(
    private val results: List<Channel> = emptyList(),
    private val fuzzy: Boolean = false,
    private val failWith: ApiException? = null,
) : SearchAccess {
    override suspend fun sources(): List<Source> = emptyList()

    override suspend fun search(
        query: String,
        sourceId: Long?,
        kind: MediaKind?,
        offset: UInt,
        limit: UInt,
    ): SearchPage {
        failWith?.let { throw it }
        return SearchPage(channels = results, offset = offset, fuzzy = fuzzy)
    }
}
