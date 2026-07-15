// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.core.corekit

import kotlinx.coroutines.flow.Flow
import uniffi.core_api.AppSettings
import uniffi.core_api.BrowseGroupPage
import uniffi.core_api.BufferingProfile
import uniffi.core_api.ChannelPage
import uniffi.core_api.Handshake
import uniffi.core_api.InterfaceDensity
import uniffi.core_api.LogLevel
import uniffi.core_api.MediaKind
import uniffi.core_api.Recent
import uniffi.core_api.SearchPage
import uniffi.core_api.Source
import uniffi.core_api.SubtitleBackground
import uniffi.core_api.SubtitleSize

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

/**
 * The narrow core surface the **settings** slice needs: the whole settings snapshot, one setter per
 * setting the app surfaces, the recents off-switch and clear, the log export, and the handshake for
 * the diagnostics versions block (PRD §6.9).
 *
 * Two ownership notes the shape encodes deliberately:
 *  - The recents off-switch belongs to the core's **recents** service, not its settings service.
 *    [settings] only *reports* `recentsEnabled`; [setRecentsEnabled] and [clearRecents] route to the
 *    owning service, so the flag has exactly one writer.
 *  - There is no EPG-window setter here, though the core has one. EPG ingest lands in a later phase,
 *    and a settings row that changes nothing the viewer can observe is a UX bug, not a feature — so
 *    the shell does not offer the window until there is a guide to window (PRD §6.6).
 */
interface SettingsAccess {
    /** Every persisted setting in one read; the settings screen's single source of truth. */
    suspend fun settings(): AppSettings

    /** Sets the global default engine, or clears it with `null` to fall back to the platform
     * default. The key is opaque here exactly as it is to the core — engine identity is the player
     * layer's concept (TECH_SPEC §8), and corekit must not depend on player-contract to name it. */
    suspend fun setDefaultEngine(engine: String?)

    suspend fun setBuffering(profile: BufferingProfile)

    suspend fun setSubtitleSize(size: SubtitleSize)

    suspend fun setSubtitleBackground(background: SubtitleBackground)

    /** Sets the UI language as a BCP-47 tag, or `null` to follow the system language. */
    suspend fun setLanguage(tag: String?)

    suspend fun setDensity(density: InterfaceDensity)

    suspend fun setRecentsRetentionDays(days: UInt)

    suspend fun setImageCacheMaxMb(megabytes: UInt)

    suspend fun setLogLevel(level: LogLevel)

    /** The recents off-switch, routed to the core's recents service — see the note above. */
    suspend fun setRecentsEnabled(enabled: Boolean)

    /** Drops the recently-watched history — see the note above. */
    suspend fun clearRecents()

    /** The buffered recent log lines for the diagnostics viewer (TECH_SPEC §4.8). */
    suspend fun exportLogs(): List<String>

    /** Core / schema / boundary versions for the diagnostics versions block (PRD §6.9). */
    fun handshake(): Handshake
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
