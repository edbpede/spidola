// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv

import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.navigation3.runtime.entryProvider
import androidx.navigation3.runtime.rememberNavBackStack
import androidx.navigation3.ui.NavDisplay
import dev.spidola.tv.core.corekit.SpidolaCore
import dev.spidola.tv.feature.browse.BrowseNavigator
import dev.spidola.tv.feature.browse.ChannelDetailScreen
import dev.spidola.tv.feature.browse.ChannelsScreen
import dev.spidola.tv.feature.browse.HomeScreen
import dev.spidola.tv.feature.browse.SourceBrowseScreen
import dev.spidola.tv.feature.search.SearchScreen
import dev.spidola.tv.feature.sources.AddSourceScreen
import dev.spidola.tv.feature.sources.SourcesScreen
import uniffi.core_api.MediaKind

/**
 * Navigation 3 host (TECH_SPEC §7): the back stack is plain observable state that the app owns,
 * rendered by [NavDisplay]. The composition root wires the [BrowseNavigator] (feature-facing
 * navigation intents) onto back-stack pushes, and hands each feature the one narrow access
 * interface it needs — the concrete [SpidolaCore] implements them all. Back pops the stack; Back
 * from the home root exits.
 */
@Composable
fun SpidolaNavHost(
    core: SpidolaCore,
    modifier: Modifier = Modifier,
) {
    val backStack = rememberNavBackStack(HomeRoute)
    val navigator =
        remember(backStack) {
            BrowseNavigator(
                openSource = { id, name -> backStack.add(SourceRoute(id, name)) },
                openChannels = { sourceId, kind, group, title ->
                    backStack.add(ChannelsRoute(sourceId, kind.name, group, title))
                },
                openChannel = { channel -> backStack.add(ChannelRoute.of(channel)) },
                openSearch = { backStack.add(SearchRoute) },
                manageSources = { backStack.add(ManageSourcesRoute) },
            )
        }

    NavDisplay(
        backStack = backStack,
        modifier = modifier,
        entryProvider =
            entryProvider {
                entry<HomeRoute> {
                    HomeScreen(access = core, navigator = navigator)
                }
                entry<SourceRoute> { route ->
                    SourceBrowseScreen(sourceId = route.sourceId, access = core, navigator = navigator)
                }
                entry<ChannelsRoute> { route ->
                    ChannelsScreen(
                        sourceId = route.sourceId,
                        kind = MediaKind.valueOf(route.kindName),
                        group = route.group,
                        access = core,
                        navigator = navigator,
                    )
                }
                entry<ChannelRoute> { route ->
                    ChannelDetailScreen(channel = route.toPlayable(), access = core)
                }
                entry<SearchRoute> {
                    SearchScreen(
                        access = core,
                        onOpenChannel = { channel -> backStack.add(ChannelRoute.of(channel)) },
                    )
                }
                entry<ManageSourcesRoute> {
                    SourcesScreen(access = core, onAddSource = { backStack.add(AddSourceRoute) })
                }
                entry<AddSourceRoute> {
                    AddSourceScreen(access = core, onFinished = { backStack.removeLastOrNull() })
                }
            },
    )
}
