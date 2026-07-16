// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.feature.browse

import dev.spidola.tv.core.corekit.CustomChannelInput
import dev.spidola.tv.core.corekit.CustomChannelsAccess
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
import org.junit.jupiter.api.BeforeEach
import org.junit.jupiter.api.Test
import uniffi.core_api.CustomChannelSummary
import uniffi.core_api.CustomGroup
import uniffi.core_api.CustomImportMode

@OptIn(ExperimentalCoroutinesApi::class)
class CustomChannelsViewModelTest {
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
    fun `manager loads ordered groups and channels`() =
        runTest(dispatcher) {
            val viewModel = CustomChannelsViewModel(FakeCustomAccess())
            advanceUntilIdle()

            val state = viewModel.state.value
            check(state is LoadState.Ready)
            assertEquals(listOf("Local"), state.value.groups.map { it.name })
            assertEquals(listOf("DR Living"), state.value.sections.flatMap { it.channels }.map { it.name })
        }

    @Test
    fun `replace import remains an explicit adapter choice`() =
        runTest(dispatcher) {
            val access = FakeCustomAccess()
            val viewModel = CustomChannelsViewModel(access)
            advanceUntilIdle()

            viewModel.import("{\"version\":1}", CustomImportMode.REPLACE)
            advanceUntilIdle()

            assertEquals(CustomImportMode.REPLACE, access.importMode)
        }
}

private class FakeCustomAccess : CustomChannelsAccess {
    private val group = CustomGroup(1L, "Local", 0L)
    var importMode: CustomImportMode? = null

    override suspend fun customGroups(): List<CustomGroup> = listOf(group)

    override suspend fun customChannels(groupId: Long?): List<CustomChannelSummary> =
        if (groupId == group.id) {
            listOf(CustomChannelSummary(2L, group.id, "DR Living", null, false, 0u, 0L))
        } else {
            emptyList()
        }

    override suspend fun createCustomGroup(name: String): Long = 3L

    override suspend fun renameCustomGroup(
        id: Long,
        name: String,
    ) = Unit

    override suspend fun deleteCustomGroup(id: Long) = Unit

    override suspend fun moveCustomGroupBefore(
        id: Long,
        anchorId: Long,
    ) = Unit

    override suspend fun moveCustomGroupAfter(
        id: Long,
        anchorId: Long,
    ) = Unit

    override suspend fun createCustomChannel(input: CustomChannelInput): Long = 4L

    override suspend fun updateCustomChannel(
        id: Long,
        input: CustomChannelInput,
    ) = Unit

    override suspend fun deleteCustomChannel(id: Long) = Unit

    override suspend fun moveCustomChannelBefore(
        id: Long,
        anchorId: Long,
    ) = Unit

    override suspend fun moveCustomChannelAfter(
        id: Long,
        anchorId: Long,
    ) = Unit

    override suspend fun exportCustomChannels(): String = "{\"version\":1}"

    override suspend fun importCustomChannels(
        contents: String,
        mode: CustomImportMode,
    ): ULong {
        importMode = mode
        return 1u
    }
}
