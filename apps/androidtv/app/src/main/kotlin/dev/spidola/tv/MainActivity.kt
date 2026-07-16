// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv

import android.app.SearchManager
import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.lifecycle.lifecycleScope
import androidx.navigation3.runtime.NavKey
import dev.spidola.tv.core.corekit.PlayableChannel
import dev.spidola.tv.core.designsystem.SpidolaTheme
import kotlinx.coroutines.launch

/**
 * The single Activity (TECH_SPEC §7). It is a thin host: it installs the theme and the
 * Navigation 3 back-stack-as-state graph, handing the graph the core catalog from the app
 * container, and owns nothing else.
 */
class MainActivity : ComponentActivity() {
    private var launchRoute: NavKey by mutableStateOf(HomeRoute)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val app = application as SpidolaApplication
        lifecycleScope.launch {
            app.bootstrap.await()
            launchRoute = intent.resolveStartRoute(app)
            setContent {
                SpidolaTheme {
                    key(launchRoute) {
                        SpidolaNavHost(
                            core = app.container.core,
                            registry = app.container.registry,
                            handoff = app.container.pairingHandoff,
                            startRoute = launchRoute,
                        )
                    }
                }
            }
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        val app = application as SpidolaApplication
        lifecycleScope.launch {
            app.bootstrap.await()
            launchRoute = intent.resolveStartRoute(app)
        }
    }
}

private suspend fun Intent.resolveStartRoute(app: SpidolaApplication): NavKey {
    if (action == Intent.ACTION_VIEW && data?.host == "channel") {
        return platformChannelRouteAfterRestart(
            sourceId = data?.getQueryParameter("sourceId"),
            identity = data?.getQueryParameter("identity"),
            cachedLookup = app.container.tvContentPublisher::channel,
            persistentLookup = app.container.core::channelByIdentity,
        )
    }
    return toStartRoute()
}

internal fun Intent.toStartRoute(channelLookup: (Long, Long) -> PlayableChannel? = { _, _ -> null }): NavKey =
    platformStartRoute(
        action = action,
        searchQuery = getStringExtra(SearchManager.QUERY),
        deepLinkHost = data?.host,
        deepLinkQuery = data?.getQueryParameter("q"),
        channelSourceId = data?.getQueryParameter("sourceId"),
        channelIdentity = data?.getQueryParameter("identity"),
        channelLookup = channelLookup,
    )

internal fun platformStartRoute(
    action: String?,
    searchQuery: String?,
    deepLinkHost: String?,
    deepLinkQuery: String?,
    channelSourceId: String? = null,
    channelIdentity: String? = null,
    channelLookup: (Long, Long) -> PlayableChannel? = { _, _ -> null },
): NavKey =
    when {
        action == Intent.ACTION_SEARCH -> SearchRoute(searchQuery.orEmpty())
        action == Intent.ACTION_VIEW && deepLinkHost == "search" -> SearchRoute(deepLinkQuery.orEmpty())
        action == Intent.ACTION_VIEW && deepLinkHost == "channel" ->
            platformChannelRoute(
                sourceId = channelSourceId,
                identity = channelIdentity,
                channelLookup = channelLookup,
            )
        else -> HomeRoute
    }

internal fun platformChannelRoute(
    sourceId: String?,
    identity: String?,
    channelLookup: (Long, Long) -> PlayableChannel?,
): NavKey {
    val parsedSource = sourceId?.toLongOrNull() ?: return HomeRoute
    val parsedIdentity = identity?.toLongOrNull() ?: return HomeRoute
    val channel = channelLookup(parsedSource, parsedIdentity) ?: return HomeRoute
    return ChannelRoute.of(channel, dev.spidola.tv.core.corekit.ZapContext.Single, 0u)
}

internal suspend fun platformChannelRouteAfterRestart(
    sourceId: String?,
    identity: String?,
    cachedLookup: (Long, Long) -> PlayableChannel?,
    persistentLookup: suspend (Long, Long) -> PlayableChannel?,
): NavKey {
    val parsedSource = sourceId?.toLongOrNull() ?: return HomeRoute
    val parsedIdentity = identity?.toLongOrNull() ?: return HomeRoute
    val channel =
        cachedLookup(parsedSource, parsedIdentity)
            ?: persistentLookup(parsedSource, parsedIdentity)
            ?: return HomeRoute
    return ChannelRoute.of(channel, dev.spidola.tv.core.corekit.ZapContext.Single, 0u)
}
