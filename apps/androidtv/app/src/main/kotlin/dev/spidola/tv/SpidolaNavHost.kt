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
import dev.spidola.tv.core.playercontract.EngineRegistry
import dev.spidola.tv.feature.browse.BrowseNavigator
import dev.spidola.tv.feature.browse.ChannelDetailScreen
import dev.spidola.tv.feature.browse.ChannelsScreen
import dev.spidola.tv.feature.browse.HomeScreen
import dev.spidola.tv.feature.browse.SourceBrowseScreen
import dev.spidola.tv.feature.playback.PlaybackScreen
import dev.spidola.tv.feature.search.SearchScreen
import dev.spidola.tv.feature.settings.DiagnosticsScreen
import dev.spidola.tv.feature.settings.SettingsNavigator
import dev.spidola.tv.feature.settings.SettingsPicker
import dev.spidola.tv.feature.settings.SettingsPickerScreen
import dev.spidola.tv.feature.settings.SettingsScreen
import dev.spidola.tv.feature.sources.AddSourceScreen
import dev.spidola.tv.feature.sources.PairingHandoff
import dev.spidola.tv.feature.sources.PairingScreen
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
    registry: EngineRegistry,
    handoff: PairingHandoff,
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
                openChannel = { channel, context, offset ->
                    backStack.add(ChannelRoute.of(channel, context, offset))
                },
                openSearch = { backStack.add(SearchRoute) },
                manageSources = { backStack.add(ManageSourcesRoute) },
                openSettings = { backStack.add(SettingsRoute) },
            )
        }
    val settingsNavigator =
        remember(backStack) {
            SettingsNavigator(
                openPicker = { picker -> backStack.add(SettingsPickerRoute(picker.name)) },
                openDiagnostics = { backStack.add(DiagnosticsRoute) },
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
                    ChannelDetailScreen(
                        channel = route.channel.toPlayable(),
                        access = core,
                        onPlay = { backStack.add(PlaybackRoute.of(route)) },
                    )
                }
                entry<PlaybackRoute> { route ->
                    PlaybackScreen(
                        channel = route.channel.toPlayable(),
                        context = route.context.toContext(),
                        offset = route.offset,
                        access = core,
                        registry = registry,
                        onExit = { backStack.removeLastOrNull() },
                    )
                }
                entry<SearchRoute> {
                    SearchScreen(
                        access = core,
                        onOpenChannel = { channel, context, offset ->
                            backStack.add(ChannelRoute.of(channel, context, offset))
                        },
                    )
                }
                entry<ManageSourcesRoute> {
                    SourcesScreen(
                        access = core,
                        onAddSource = { backStack.add(AddSourceRoute) },
                        onPairPhone = { backStack.add(PairingRoute) },
                    )
                }
                entry<AddSourceRoute> {
                    // Claimed once, as the screen's entry is composed: a submission pre-fills this
                    // form exactly once, and re-entering add-source later starts blank rather than
                    // re-filling someone's account.
                    val prefill = remember { handoff.take() }
                    AddSourceScreen(
                        access = core,
                        onFinished = { backStack.removeLastOrNull() },
                        prefill = prefill,
                    )
                }
                entry<PairingRoute> {
                    PairingScreen(
                        access = core,
                        handoff = handoff,
                        // Replace rather than push: the pairing screen's job is done, its server is
                        // stopped, and Back from the pre-filled form should land on the sources list
                        // rather than restart a server the viewer already finished with.
                        onSubmissionReady = {
                            backStack.removeLastOrNull()
                            backStack.add(AddSourceRoute)
                        },
                        onGoBack = { backStack.removeLastOrNull() },
                    )
                }
                entry<SettingsRoute> {
                    SettingsScreen(
                        access = core,
                        navigator = settingsNavigator,
                        onGoBack = { backStack.removeLastOrNull() },
                    )
                }
                entry<SettingsPickerRoute> { route ->
                    SettingsPickerScreen(
                        picker = SettingsPicker.valueOf(route.pickerName),
                        access = core,
                        onClose = { backStack.removeLastOrNull() },
                    )
                }
                entry<DiagnosticsRoute> {
                    DiagnosticsScreen(
                        access = core,
                        // The one place the app's own version is known; the feature module must not
                        // reach up into the shell's BuildConfig to read it.
                        appVersion = BuildConfig.VERSION_NAME,
                        onOpenLogLevel = { backStack.add(SettingsPickerRoute(SettingsPicker.LOG_LEVEL.name)) },
                        onGoBack = { backStack.removeLastOrNull() },
                    )
                }
            },
    )
}
