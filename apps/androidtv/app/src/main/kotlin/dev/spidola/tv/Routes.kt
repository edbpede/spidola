// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv

import androidx.navigation3.runtime.NavKey
import dev.spidola.tv.core.corekit.PlayableChannel
import kotlinx.serialization.Serializable

/**
 * The Navigation 3 route set (TECH_SPEC §7): every destination is a serializable [NavKey] the back
 * stack holds as plain state. Payloads are primitives (the media kind travels as its enum name) so
 * the whole back stack restores across process death without a custom serializer.
 */
@Serializable
data object HomeRoute : NavKey

@Serializable
data class SourceRoute(
    val sourceId: Long,
    val sourceName: String,
) : NavKey

@Serializable
data class ChannelsRoute(
    val sourceId: Long,
    val kindName: String,
    val group: String?,
    val title: String,
) : NavKey

@Serializable
data class ChannelRoute(
    val sourceId: Long,
    val identity: Long,
    val name: String,
    val group: String?,
    val logo: String?,
    val locator: String,
) : NavKey {
    fun toPlayable(): PlayableChannel =
        PlayableChannel(
            sourceId = sourceId,
            identity = identity,
            name = name,
            group = group,
            logo = logo,
            locator = locator,
        )

    companion object {
        fun of(channel: PlayableChannel): ChannelRoute =
            ChannelRoute(
                sourceId = channel.sourceId,
                identity = channel.identity,
                name = channel.name,
                group = channel.group,
                logo = channel.logo,
                locator = channel.locator,
            )
    }
}

@Serializable
data object SearchRoute : NavKey

@Serializable
data object ManageSourcesRoute : NavKey

@Serializable
data object AddSourceRoute : NavKey
