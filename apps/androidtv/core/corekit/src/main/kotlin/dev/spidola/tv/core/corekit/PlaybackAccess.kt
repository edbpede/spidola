// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.core.corekit

import uniffi.core_api.MediaKind

/**
 * The list a channel was played from, and therefore the ring that D-pad up/down zaps through
 * (PRD §8.4). It names the *query*, not its results: the core stays the single source of truth, and
 * a 50k-channel ring never crosses the FFI as a list.
 *
 * Every arm maps onto a paged core query the shell already issues, so zapping costs one three-row
 * page rather than a new core surface.
 */
sealed interface ZapContext {
    /** Played from the browse drill-down — zaps through that category. */
    data class Group(
        val sourceId: Long,
        val kind: MediaKind,
        val group: String?,
    ) : ZapContext

    /** Played from the favourites row — zaps through favourites. */
    data object Favorites : ZapContext

    /** Played from search — zaps through the result set. */
    data class Search(
        val query: String,
        val sourceId: Long?,
        val kind: MediaKind?,
    ) : ZapContext

    /**
     * Played from somewhere with no natural ring (a recent). Zap is unavailable and the strip shows
     * no peek — an honest absence rather than a ring invented from one row.
     */
    data object Single : ZapContext
}

/**
 * The channel being played and its immediate neighbours — everything the channel strip's
 * adjacent-peek and the zap path need, in one page (PRD §8.5).
 */
data class ZapWindow(
    val previous: PlayableChannel?,
    val current: PlayableChannel,
    val next: PlayableChannel?,
    /** [current]'s position in the ring; zapping moves this by one. */
    val offset: UInt,
    /**
     * The ring's length, so the strip can show position. `null` when the ring's length is not
     * knowable — the search query is scored and paged without a count, so a search ring reports an
     * honest "unknown" rather than a total invented from the current page.
     */
    val total: ULong?,
)

/**
 * The narrow core surface the **playback** slice needs: the zap ring, the persisted engine overrides
 * the selection policy reads, the engine-neutral playback settings, and the play-time recents
 * record.
 *
 * Engine overrides are **opaque strings** here, never `EngineId`: corekit must not depend on
 * player-contract (engine identity is the player layer's concept, and the core already persists it
 * as an opaque key — TECH_SPEC §8). The playback slice, which depends on both, does the mapping.
 */
interface PlaybackAccess {
    /**
     * Resolves the channel at [offset] in [context] plus its neighbours. Returns `null` when the
     * context has no ring ([ZapContext.Single]) or the ring no longer has a row there — a catalog
     * refresh can move offsets under a playing channel, and inventing a neighbour would zap the
     * viewer somewhere they did not ask for.
     */
    suspend fun zapWindow(
        context: ZapContext,
        offset: UInt,
    ): ZapWindow?

    /** The "remember for this channel" engine choice, if the viewer set one. */
    suspend fun channelEngine(
        sourceId: Long,
        identity: Long,
    ): String?

    /** Sets or (with a `null` engine) clears the per-channel engine choice. */
    suspend fun setChannelEngine(
        sourceId: Long,
        identity: Long,
        engine: String?,
    )

    /** The per-source engine choice, if set. */
    suspend fun sourceEngine(sourceId: Long): String?

    /** The engine-neutral buffering profile, as its raw key; `null` means the app default. */
    suspend fun bufferingProfile(): String?

    suspend fun setBufferingProfile(profile: String)

    suspend fun recordRecent(channel: PlayableChannel)
}

/**
 * The settings keys the playback slice owns.
 *
 * Engine overrides live in the settings table rather than `channels.preferred_engine` on purpose:
 * the channels table is replaced wholesale by the staging-and-swap refresh (TECH_SPEC §4.4), so a
 * choice stored there would silently vanish on the next refresh. Favourites and hidden survive
 * refresh by living in their own tables keyed on the stable identity hash; a remembered engine is
 * the same kind of durable per-channel fact and is keyed the same way.
 */
internal object PlaybackSettingKey {
    fun channelEngine(
        sourceId: Long,
        identity: Long,
    ): String = "player.engine.channel.$sourceId.$identity"

    fun sourceEngine(sourceId: Long): String = "player.engine.source.$sourceId"

    const val BUFFERING_PROFILE = "player.buffering"
}
