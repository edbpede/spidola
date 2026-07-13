// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.feature.browse

import kotlinx.collections.immutable.ImmutableList

/** The browse screen's state. A closed set, matched exhaustively by the composable. */
sealed interface BrowseUiState {
    data object Loading : BrowseUiState

    data object Empty : BrowseUiState

    data class Ready(val channels: ImmutableList<ChannelItem>) : BrowseUiState

    data class Error(val message: String) : BrowseUiState
}

/**
 * A single channel row's display data. [key] is the stable per-source identity (not the rowid),
 * so focus restoration survives a catalog refresh (Compose focusRestorer needs stable keys).
 */
data class ChannelItem(
    val key: Long,
    val name: String,
    val group: String?,
)
