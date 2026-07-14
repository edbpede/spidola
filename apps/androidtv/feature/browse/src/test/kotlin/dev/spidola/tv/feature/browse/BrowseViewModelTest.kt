// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.feature.browse

import dev.spidola.tv.core.corekit.BrowseAccess
import dev.spidola.tv.core.corekit.HomeAccess
import dev.spidola.tv.core.corekit.LoadState
import dev.spidola.tv.core.corekit.PlayableChannel
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.jupiter.api.AfterEach
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertFalse
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.BeforeEach
import org.junit.jupiter.api.Test
import uniffi.core_api.ApiException
import uniffi.core_api.BrowseGroup
import uniffi.core_api.BrowseGroupPage
import uniffi.core_api.Channel
import uniffi.core_api.ChannelOverrides
import uniffi.core_api.ChannelPage
import uniffi.core_api.MediaKind
import uniffi.core_api.Recent
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
    fun `home is empty without enabled sources`() =
        runTest(dispatcher) {
            val viewModel = HomeViewModel(FakeAccess(sources = emptyList()))
            advanceUntilIdle()
            assertEquals(LoadState.Empty, viewModel.state.value)
        }

    @Test
    fun `home lists favorites and recents`() =
        runTest(dispatcher) {
            val viewModel =
                HomeViewModel(
                    FakeAccess(
                        sources = listOf(source(1L)),
                        favorites = listOf(channel(10L, "BBC")),
                        recents = listOf(recent(11L, "CNN")),
                    ),
                )
            advanceUntilIdle()
            val state = viewModel.state.value
            check(state is LoadState.Ready)
            assertEquals(listOf("BBC"), state.value.favorites.map { it.name })
            assertEquals(listOf("CNN"), state.value.recents.map { it.name })
        }

    @Test
    fun `home hides recents when the off-switch is set`() =
        runTest(dispatcher) {
            val viewModel =
                HomeViewModel(
                    FakeAccess(
                        sources = listOf(source(1L)),
                        recents = listOf(recent(11L, "CNN")),
                        recentsEnabled = false,
                    ),
                )
            advanceUntilIdle()
            val state = viewModel.state.value
            check(state is LoadState.Ready)
            assertTrue(state.value.recents.isEmpty())
        }

    @Test
    fun `home surfaces an actionable error`() =
        runTest(dispatcher) {
            val viewModel = HomeViewModel(FakeAccess(sources = emptyList(), failWith = ApiException.StorageCorrupt()))
            advanceUntilIdle()
            val state = viewModel.state.value
            check(state is LoadState.Failed)
            assertTrue(state.error.actions.isNotEmpty())
        }

    @Test
    fun `source browse lists groups`() =
        runTest(dispatcher) {
            val viewModel =
                SourceBrowseViewModel(
                    1L,
                    FakeAccess(
                        sources = listOf(source(1L)),
                        groups = listOf(BrowseGroup("News", 2uL), BrowseGroup(null, 1uL)),
                    ),
                )
            advanceUntilIdle()
            val state = viewModel.state.value
            check(state is LoadState.Ready)
            assertEquals(listOf(2uL, 1uL), state.value.groups.map { it.channelCount })
        }

    @Test
    fun `channels toggle favorite then hide`() =
        runTest(dispatcher) {
            val viewModel =
                ChannelsViewModel(
                    1L,
                    MediaKind.LIVE,
                    "Fixture",
                    FakeAccess(sources = listOf(source(1L)), groupChannels = listOf(channel(42L, "Fixture"))),
                )
            advanceUntilIdle()
            val ready = viewModel.state.value
            check(ready is LoadState.Ready)
            assertFalse(ready.value.first().isFavorite)

            viewModel.toggleFavorite(ready.value.first())
            advanceUntilIdle()
            val afterFav = viewModel.state.value
            check(afterFav is LoadState.Ready)
            assertTrue(afterFav.value.first().isFavorite)

            viewModel.hide(afterFav.value.first())
            advanceUntilIdle()
            assertEquals(LoadState.Empty, viewModel.state.value)
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
        identity: Long,
        name: String,
    ): Channel =
        Channel(
            id = identity,
            sourceId = 1L,
            identity = identity,
            name = name,
            groupTitle = "Fixture",
            logo = null,
            locator = "http://host.example/$identity.ts",
            kind = MediaKind.LIVE,
            categoryId = null,
            overrides = ChannelOverrides(userAgent = null, headers = emptyList(), preferredEngine = null),
        )

    private fun recent(
        identity: Long,
        name: String,
    ): Recent =
        Recent(
            sourceId = 1L,
            identity = identity,
            name = name,
            locator = "http://host.example/$identity.ts",
            playedAtUnix = 1000L,
            positionSecs = null,
        )
}

/** A fake implementing both [HomeAccess] and [BrowseAccess], so the browse view models are tested
 * without the real core (TECH_SPEC §10). */
private class FakeAccess(
    private val sources: List<Source>,
    private val favorites: List<Channel> = emptyList(),
    private val recents: List<Recent> = emptyList(),
    private val recentsEnabled: Boolean = true,
    private val kinds: List<MediaKind> = listOf(MediaKind.LIVE),
    private val groups: List<BrowseGroup> = emptyList(),
    private val groupChannels: List<Channel> = emptyList(),
    private val failWith: ApiException? = null,
) : HomeAccess,
    BrowseAccess {
    private var favoriteIds = mutableSetOf<Long>()
    private val hiddenIds = mutableSetOf<Long>()

    private fun check() {
        failWith?.let { throw it }
    }

    override suspend fun sources(): List<Source> {
        check()
        return sources
    }

    override suspend fun favoriteChannels(
        offset: UInt,
        limit: UInt,
    ): ChannelPage {
        check()
        return ChannelPage(channels = favorites, offset = offset, total = favorites.size.toULong())
    }

    override suspend fun recents(limit: UInt): List<Recent> {
        check()
        return recents
    }

    override suspend fun recentsEnabled(): Boolean {
        check()
        return recentsEnabled
    }

    override suspend fun setRecentsEnabled(enabled: Boolean) = check()

    override suspend fun clearRecents() = check()

    override suspend fun recordRecent(channel: PlayableChannel) = check()

    override suspend fun kinds(sourceId: Long): List<MediaKind> {
        check()
        return kinds
    }

    override suspend fun groups(
        sourceId: Long,
        kind: MediaKind,
        offset: UInt,
        limit: UInt,
    ): BrowseGroupPage {
        check()
        return BrowseGroupPage(groups = groups, offset = offset, total = groups.size.toULong())
    }

    override suspend fun channelsInGroup(
        sourceId: Long,
        kind: MediaKind,
        group: String?,
        offset: UInt,
        limit: UInt,
    ): ChannelPage {
        check()
        val visible = groupChannels.filter { it.identity !in hiddenIds }
        return ChannelPage(channels = visible, offset = offset, total = visible.size.toULong())
    }

    override suspend fun isFavorite(
        sourceId: Long,
        identity: Long,
    ): Boolean {
        check()
        return identity in favoriteIds
    }

    override suspend fun setFavorite(
        sourceId: Long,
        identity: Long,
        favorite: Boolean,
    ) {
        check()
        if (favorite) favoriteIds.add(identity) else favoriteIds.remove(identity)
    }

    override suspend fun favoriteIdentities(sourceId: Long): List<Long> {
        check()
        return favoriteIds.toList()
    }

    override suspend fun isHidden(
        sourceId: Long,
        identity: Long,
    ): Boolean {
        check()
        return identity in hiddenIds
    }

    override suspend fun setHidden(
        sourceId: Long,
        identity: Long,
        hidden: Boolean,
    ) {
        check()
        if (hidden) hiddenIds.add(identity) else hiddenIds.remove(identity)
    }
}
