// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv

import androidx.navigation3.runtime.NavKey
import dev.spidola.tv.core.corekit.PlayableChannel
import dev.spidola.tv.core.corekit.ZapContext
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import uniffi.core_api.MediaKind

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

/**
 * A [PlayableChannel] flattened into primitives for the back stack. Its own type because both the
 * detail and the playback route carry a channel, and a second copy of this mapping could drift.
 *
 * The kind travels as its enum name, like every other kind in this file, and stays nullable: a
 * channel opened from a recent never had one to carry, so the absence restores as an absence rather
 * than as a claim about what the channel plays.
 */
@Serializable
data class ChannelPayload(
    val sourceId: Long,
    val identity: Long,
    val name: String,
    val group: String?,
    val logo: String?,
    val locator: String,
    val kindName: String?,
) {
    fun toPlayable(): PlayableChannel =
        PlayableChannel(
            sourceId = sourceId,
            identity = identity,
            name = name,
            group = group,
            logo = logo,
            locator = locator,
            kind = kindName?.let(MediaKind::valueOf),
        )

    companion object {
        fun of(channel: PlayableChannel): ChannelPayload =
            ChannelPayload(
                sourceId = channel.sourceId,
                identity = channel.identity,
                name = channel.name,
                group = channel.group,
                logo = channel.logo,
                locator = channel.locator,
                kindName = channel.kind?.name,
            )
    }
}

/**
 * A channel's detail screen, plus the ring it was chosen from — carried so that pressing Play hands
 * playback the zap context the viewer's own path implies (PRD §8.4).
 */
@Serializable
data class ChannelRoute(
    val channel: ChannelPayload,
    val context: ZapContextRoute,
    /** The channel's absolute position in [context]'s ring. */
    val offset: UInt,
) : NavKey {
    companion object {
        fun of(
            channel: PlayableChannel,
            context: ZapContext,
            offset: UInt,
        ): ChannelRoute = ChannelRoute(ChannelPayload.of(channel), ZapContextRoute.of(context), offset)
    }
}

/**
 * A [ZapContext] in a form the back stack can hold (PRD §8.4).
 *
 * The context is a sealed hierarchy in corekit and cannot be annotated there — its media kind comes
 * from the generated FFI bindings — so it travels as this mirror, with the kind as its enum name
 * exactly like [ChannelsRoute]. Sealed rather than a discriminator plus nullable fields: each arm
 * then carries exactly the values that arm requires, so restoring one cannot produce a half-built
 * context that only `!!` could unpack.
 */
@Serializable
sealed interface ZapContextRoute {
    @Serializable
    @SerialName("group")
    data class Group(
        val sourceId: Long,
        val kindName: String,
        val group: String?,
    ) : ZapContextRoute

    @Serializable
    @SerialName("favorites")
    data object Favorites : ZapContextRoute

    @Serializable
    @SerialName("search")
    data class Search(
        val query: String,
        val sourceId: Long?,
        val kindName: String?,
    ) : ZapContextRoute

    @Serializable
    @SerialName("single")
    data object Single : ZapContextRoute

    fun toContext(): ZapContext =
        when (this) {
            is Group -> ZapContext.Group(sourceId, MediaKind.valueOf(kindName), group)
            Favorites -> ZapContext.Favorites
            is Search -> ZapContext.Search(query, sourceId, kindName?.let(MediaKind::valueOf))
            Single -> ZapContext.Single
        }

    companion object {
        fun of(context: ZapContext): ZapContextRoute =
            when (context) {
                is ZapContext.Group -> Group(context.sourceId, context.kind.name, context.group)
                ZapContext.Favorites -> Favorites
                is ZapContext.Search -> Search(context.query, context.sourceId, context.kind?.name)
                ZapContext.Single -> Single
            }
    }
}

/**
 * The playing channel plus the ring it zaps through, so playback resolves neighbours from the path
 * the viewer actually took rather than a ring invented at play time (PRD §8.4).
 */
@Serializable
data class PlaybackRoute(
    val channel: ChannelPayload,
    val context: ZapContextRoute,
    /** The channel's absolute position in [context]'s ring; zapping moves this by one. */
    val offset: UInt,
) : NavKey {
    companion object {
        /** Plays what the detail screen is showing, keeping the ring it was opened with. */
        fun of(route: ChannelRoute): PlaybackRoute = PlaybackRoute(route.channel, route.context, route.offset)
    }
}

/**
 * A custom channel playback destination. Only its opaque database ID and presentation metadata are
 * saved with the back stack; the sealed locator and request overrides are resolved at play time.
 */
@Serializable
data class CustomPlaybackRoute(
    val id: Long,
    val name: String,
    val logo: String?,
) : NavKey

@Serializable
data class SearchRoute(
    val initialQuery: String = "",
) : NavKey

@Serializable
data object FavoriteLineupRoute : NavKey

@Serializable
data object CustomChannelsRoute : NavKey

@Serializable
data object ManageSourcesRoute : NavKey

/**
 * The add-source form. Carries no payload on purpose: what a phone submits over pairing includes an
 * Xtream password, and this back stack is serialized into saved instance state — a payload here
 * would write that credential to disk. The submission travels in memory instead
 * (`PairingHandoff`, TECH_SPEC §12).
 */
@Serializable
data object AddSourceRoute : NavKey

@Serializable
data object PairingRoute : NavKey

@Serializable
data object SettingsRoute : NavKey

/**
 * The option list for one setting (PRD §6.9). The picker travels as its enum name, like every other
 * enum in this file, so the whole back stack restores across process death without a custom
 * serializer.
 */
@Serializable
data class SettingsPickerRoute(
    val pickerName: String,
) : NavKey

@Serializable
data object DiagnosticsRoute : NavKey

@Serializable
data object GuideRoute : NavKey

@Serializable
data object AboutRoute : NavKey
