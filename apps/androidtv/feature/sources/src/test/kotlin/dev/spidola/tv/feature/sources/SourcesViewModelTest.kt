// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.feature.sources

import dev.spidola.tv.core.corekit.ImportEvent
import dev.spidola.tv.core.corekit.LoadState
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.jupiter.api.AfterEach
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertNotNull
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.BeforeEach
import org.junit.jupiter.api.Test
import uniffi.core_api.ApiException
import uniffi.core_api.ImportOutcome
import uniffi.core_api.Source
import uniffi.core_api.SourceCommon

@OptIn(ExperimentalCoroutinesApi::class)
class SourcesViewModelTest {
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
    fun `add requires a name`() =
        runTest(dispatcher) {
            val viewModel = AddSourceViewModel(FakeSourcesAccess())
            viewModel.submit(
                AddSourceForm(
                    AddSourceMode.URL,
                    name = "",
                    url = "http://x/list.m3u",
                    content = "",
                    userAgent = "",
                    acceptInvalidTls = false,
                ),
            )
            advanceUntilIdle()
            assertNotNull(viewModel.validation.value)
            assertEquals(AddSourceState.Editing, viewModel.state.value)
        }

    @Test
    fun `add url imports and summarizes`() =
        runTest(dispatcher) {
            val access =
                FakeSourcesAccess(
                    importResult =
                        ImportEvent.Complete(
                            ImportOutcome(
                                inserted = 1240uL,
                                duplicatesDropped = 0uL,
                                emitted = 1240uL,
                                skipped = 3uL,
                                invalid = 0uL,
                            ),
                        ),
                )
            val viewModel = AddSourceViewModel(access)
            viewModel.submit(
                AddSourceForm(
                    AddSourceMode.URL,
                    name = "Home",
                    url = "http://x/list.m3u",
                    content = "",
                    userAgent = "",
                    acceptInvalidTls = false,
                ),
            )
            advanceUntilIdle()
            val state = viewModel.state.value
            check(state is AddSourceState.Done)
            assertEquals(1240uL, state.outcome.inserted)
        }

    @Test
    fun `add url failure surfaces an actionable error`() =
        runTest(dispatcher) {
            val access = FakeSourcesAccess(importResult = ImportEvent.Failed(ApiException.NetworkUnreachable()))
            val viewModel = AddSourceViewModel(access)
            viewModel.submit(
                AddSourceForm(
                    AddSourceMode.URL,
                    name = "Home",
                    url = "http://x/list.m3u",
                    content = "",
                    userAgent = "",
                    acceptInvalidTls = false,
                ),
            )
            advanceUntilIdle()
            val state = viewModel.state.value
            check(state is AddSourceState.Failed)
            assertTrue(state.error.actions.isNotEmpty())
            // The source was created before the import ran, so a failed import must drop it again —
            // otherwise an empty source litters the list and a retry adds a duplicate.
            assertEquals(listOf(100L), access.deletedIds)
        }

    @Test
    fun `list loads and enable-disable records then reloads`() =
        runTest(dispatcher) {
            val access = FakeSourcesAccess(sources = listOf(source(1L, "Home", enabled = true)))
            val viewModel = SourcesViewModel(access)
            advanceUntilIdle()
            val state = viewModel.state.value
            check(state is LoadState.Ready)
            assertEquals(1, state.value.size)

            viewModel.setEnabled(1L, false)
            advanceUntilIdle()
            assertEquals(1L to false, access.lastEnabled)
        }

    @Test
    fun `delete records and reloads`() =
        runTest(dispatcher) {
            val access = FakeSourcesAccess(sources = listOf(source(1L, "Home", enabled = true)))
            val viewModel = SourcesViewModel(access)
            advanceUntilIdle()
            viewModel.delete(1L)
            advanceUntilIdle()
            assertEquals(listOf(1L), access.deletedIds)
        }

    private fun source(
        id: Long,
        name: String,
        enabled: Boolean,
    ): Source =
        Source.M3uUrl(
            id = id,
            common = SourceCommon(name = name, enabled = enabled, autoRefreshSecs = null),
            hasUserAgent = false,
            acceptInvalidTls = false,
        )
}
