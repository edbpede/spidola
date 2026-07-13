// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv

import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.navigation3.runtime.entryProvider
import androidx.navigation3.runtime.rememberNavBackStack
import androidx.navigation3.ui.NavDisplay
import dev.spidola.tv.core.corekit.CatalogAccess
import dev.spidola.tv.feature.browse.BrowseScreen

/**
 * Navigation 3 host (TECH_SPEC §7): the back stack is plain observable state that the app owns,
 * rendered by [NavDisplay]. Popping is `backStack.removeLastOrNull()`; there is nothing to pop
 * from the root in the skeleton, so Back exits.
 */
@Composable
fun SpidolaNavHost(
    catalog: CatalogAccess,
    modifier: Modifier = Modifier,
) {
    val backStack = rememberNavBackStack(BrowseRoute)
    NavDisplay(
        backStack = backStack,
        modifier = modifier,
        entryProvider =
            entryProvider {
                entry<BrowseRoute> {
                    BrowseScreen(catalog = catalog, modifier = Modifier.fillMaxSize())
                }
            },
    )
}
