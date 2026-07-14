// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.core.corekit

import kotlinx.coroutines.flow.Flow
import uniffi.core_api.BrowseGroupPage
import uniffi.core_api.ChannelPage
import uniffi.core_api.MediaKind
import uniffi.core_api.Recent
import uniffi.core_api.SearchPage
import uniffi.core_api.Source

/**
 * The narrow core surface the **sources** slice needs (add / list / manage / import). Feature code
 * depends on this interface, never the concrete [SpidolaCore], so its view models are unit-tested
 * against a fake (TECH_SPEC §10). [SpidolaCore] is the sole production implementation.
 */
interface SourcesAccess {
    suspend fun sources(): List<Source>

    suspend fun addM3uUrl(
        name: String,
        url: String,
        userAgent: String?,
        acceptInvalidTls: Boolean,
    ): Source

    suspend fun addM3uFile(name: String): Source

    suspend fun rename(
        id: Long,
        name: String,
    )

    suspend fun setEnabled(
        id: Long,
        enabled: Boolean,
    )

    suspend fun setAutoRefresh(
        id: Long,
        secs: UInt?,
    )

    suspend fun deleteSource(id: Long)

    /** Fetches (over HTTP) and imports an M3U-by-URL source, streaming progress then one terminal
     * event. Cancelling the collecting scope cancels the core task at its next batch boundary. */
    fun importUrl(id: Long): Flow<ImportEvent>

    /** Imports an M3U-from-file source from already-read [content] (SAF/picked file or pasted
     * text), streaming progress then one terminal event. */
    fun importContent(
        id: Long,
        content: String,
    ): Flow<ImportEvent>
}

/**
 * The narrow core surface the **browse** slice needs: the source → type → category → channel
 * drill-down (paged by contract), plus the per-channel context actions (favorite, hide) and the
 * play-time recents record.
 */
interface BrowseAccess {
    suspend fun sources(): List<Source>

    suspend fun kinds(sourceId: Long): List<MediaKind>

    suspend fun groups(
        sourceId: Long,
        kind: MediaKind,
        offset: UInt,
        limit: UInt,
    ): BrowseGroupPage

    suspend fun channelsInGroup(
        sourceId: Long,
        kind: MediaKind,
        group: String?,
        offset: UInt,
        limit: UInt,
    ): ChannelPage

    suspend fun isFavorite(
        sourceId: Long,
        identity: Long,
    ): Boolean

    suspend fun setFavorite(
        sourceId: Long,
        identity: Long,
        favorite: Boolean,
    )

    /** The stable identities of a source's favorites, so a channel list marks them in one query. */
    suspend fun favoriteIdentities(sourceId: Long): List<Long>

    suspend fun isHidden(
        sourceId: Long,
        identity: Long,
    ): Boolean

    suspend fun setHidden(
        sourceId: Long,
        identity: Long,
        hidden: Boolean,
    )

    suspend fun recordRecent(channel: PlayableChannel)
}

/** The narrow core surface the **search** slice needs: the ranked, paged query plus the source
 * list for the source filter. */
interface SearchAccess {
    suspend fun sources(): List<Source>

    suspend fun search(
        query: String,
        sourceId: Long?,
        kind: MediaKind?,
        offset: UInt,
        limit: UInt,
    ): SearchPage
}

/** The narrow core surface the **home** screen needs: the favorites row, the recents row with its
 * off-switch, and the enabled source list to browse into. */
interface HomeAccess {
    suspend fun sources(): List<Source>

    suspend fun favoriteChannels(
        offset: UInt,
        limit: UInt,
    ): ChannelPage

    suspend fun recents(limit: UInt): List<Recent>

    suspend fun recentsEnabled(): Boolean

    suspend fun setRecentsEnabled(enabled: Boolean)

    suspend fun clearRecents()

    suspend fun recordRecent(channel: PlayableChannel)
}
