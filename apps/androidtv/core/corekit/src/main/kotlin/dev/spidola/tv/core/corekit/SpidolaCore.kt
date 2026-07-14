// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.core.corekit

import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.callbackFlow
import uniffi.core_api.ApiException
import uniffi.core_api.BrowseGroupPage
import uniffi.core_api.ChannelPage
import uniffi.core_api.Core
import uniffi.core_api.CoreConfig
import uniffi.core_api.Handshake
import uniffi.core_api.ImportListener
import uniffi.core_api.ImportOutcome
import uniffi.core_api.ImportProgress
import uniffi.core_api.LogSink
import uniffi.core_api.MediaKind
import uniffi.core_api.Recent
import uniffi.core_api.SearchPage
import uniffi.core_api.SecretStore
import uniffi.core_api.Source
import uniffi.core_api.TaskHandle
import uniffi.core_api.uniffiEnsureInitialized

/**
 * Reads the source list and a source's channel catalog one page at a time (paged by contract,
 * TECH_SPEC §5). A narrow surface so a view-model can be unit-tested against a fake instead of the
 * real core.
 */
interface CatalogAccess {
    suspend fun sources(): List<Source>

    suspend fun page(
        sourceId: Long,
        offset: UInt,
        limit: UInt,
    ): ChannelPage
}

/** The stable rowid of a source, regardless of its kind. */
val Source.id: Long
    get() =
        when (this) {
            is Source.M3uUrl -> id
            is Source.M3uFile -> id
            is Source.Xtream -> id
        }

/** The user-facing name shared by every source kind. */
val Source.name: String
    get() =
        when (this) {
            is Source.M3uUrl -> common.name
            is Source.M3uFile -> common.name
            is Source.Xtream -> common.name
        }

/** The common per-source settings (enabled, auto-refresh) shared by every kind. */
val Source.common
    get() =
        when (this) {
            is Source.M3uUrl -> common
            is Source.M3uFile -> common
            is Source.Xtream -> common
        }

/** Whether this source can be refreshed from a URL. File sources are import-once. */
val Source.isRefreshable: Boolean
    get() =
        when (this) {
            is Source.M3uUrl -> true
            is Source.Xtream -> true
            is Source.M3uFile -> false
        }

/** A couch-legible one-word description of the source kind, for the sources list. */
val Source.kindLabel: String
    get() =
        when (this) {
            is Source.M3uUrl -> "Playlist URL"
            is Source.M3uFile -> "Playlist file"
            is Source.Xtream -> "Xtream account"
        }

/** One event from a running import; the stream terminates on [Complete] or [Failed]. */
sealed interface ImportEvent {
    data class Progress(
        val progress: ImportProgress,
    ) : ImportEvent

    data class Complete(
        val outcome: ImportOutcome,
    ) : ImportEvent

    data class Failed(
        val error: ApiException,
    ) : ImportEvent
}

/**
 * The single Kotlin-side handle on the Rust core (TECH_SPEC §5, §7). It wraps the generated [Core],
 * implements the narrow per-feature access interfaces the vertical slices depend on, and bridges
 * the import callback interface into a cold [Flow] whose cancellation reaches all the way to the
 * core's task handle (departed screen ⇒ scope ⇒ core task handle).
 */
class SpidolaCore private constructor(
    private val core: Core,
) : CatalogAccess,
    SourcesAccess,
    BrowseAccess,
    SearchAccess,
    HomeAccess,
    PlaybackAccess {
    /** The startup handshake (core / schema / boundary versions), checked before first use. */
    fun handshake(): Handshake = core.handshake()

    // ---- Sources ----

    override suspend fun sources(): List<Source> = core.sources().list()

    override suspend fun addM3uUrl(
        name: String,
        url: String,
        userAgent: String?,
        acceptInvalidTls: Boolean,
    ): Source = core.sources().addM3uUrl(name, url, userAgent, acceptInvalidTls)

    /** Convenience for the fixture seeder and simple add flows (no user-agent, platform TLS). */
    suspend fun addM3uUrl(
        name: String,
        url: String,
    ): Source = addM3uUrl(name, url, null, false)

    override suspend fun addM3uFile(name: String): Source = core.sources().addM3uFile(name)

    override suspend fun rename(
        id: Long,
        name: String,
    ) = core.sources().rename(id, name)

    override suspend fun setEnabled(
        id: Long,
        enabled: Boolean,
    ) = core.sources().setEnabled(id, enabled)

    override suspend fun setAutoRefresh(
        id: Long,
        secs: UInt?,
    ) = core.sources().setAutoRefresh(id, secs)

    override suspend fun deleteSource(id: Long) = core.sources().delete(id)

    override fun importUrl(id: Long): Flow<ImportEvent> {
        return importFlow { listener -> core.sources().refresh(id, listener) }
    }

    override fun importContent(
        id: Long,
        content: String,
    ): Flow<ImportEvent> = importFlow { listener -> core.sources().importM3uContent(id, content, listener) }

    /** Kept for the M0 fixture seeder, which imports an M3U-by-URL source. Same as [importUrl]. */
    fun import(sourceId: Long): Flow<ImportEvent> = importUrl(sourceId)

    private fun importFlow(start: (ImportListener) -> TaskHandle): Flow<ImportEvent> =
        callbackFlow {
            val listener =
                object : ImportListener {
                    override fun onProgress(progress: ImportProgress) {
                        trySend(ImportEvent.Progress(progress))
                    }

                    override fun onComplete(outcome: ImportOutcome) {
                        trySend(ImportEvent.Complete(outcome))
                        close()
                    }

                    override fun onFailed(error: ApiException) {
                        trySend(ImportEvent.Failed(error))
                        close()
                    }
                }
            val handle = start(listener)
            awaitClose { handle.cancel() }
        }

    // ---- Catalog / browse ----

    override suspend fun page(
        sourceId: Long,
        offset: UInt,
        limit: UInt,
    ): ChannelPage = core.catalog().channels(sourceId, offset, limit)

    override suspend fun kinds(sourceId: Long): List<MediaKind> = core.catalog().kinds(sourceId)

    override suspend fun groups(
        sourceId: Long,
        kind: MediaKind,
        offset: UInt,
        limit: UInt,
    ): BrowseGroupPage = core.catalog().groups(sourceId, kind, offset, limit)

    override suspend fun channelsInGroup(
        sourceId: Long,
        kind: MediaKind,
        group: String?,
        offset: UInt,
        limit: UInt,
    ): ChannelPage = core.catalog().channelsInGroup(sourceId, kind, group, offset, limit)

    override suspend fun isHidden(
        sourceId: Long,
        identity: Long,
    ): Boolean = core.catalog().isHidden(sourceId, identity)

    override suspend fun setHidden(
        sourceId: Long,
        identity: Long,
        hidden: Boolean,
    ) = core.catalog().setHidden(sourceId, identity, hidden)

    // ---- Favorites ----

    override suspend fun isFavorite(
        sourceId: Long,
        identity: Long,
    ): Boolean = core.favorites().isFavorite(sourceId, identity)

    override suspend fun setFavorite(
        sourceId: Long,
        identity: Long,
        favorite: Boolean,
    ) {
        if (favorite) {
            core.favorites().add(sourceId, identity)
        } else {
            core.favorites().remove(sourceId, identity)
        }
    }

    override suspend fun favoriteIdentities(sourceId: Long): List<Long> {
        return core.favorites().list(sourceId).map { it.identity }
    }

    override suspend fun favoriteChannels(
        offset: UInt,
        limit: UInt,
    ): ChannelPage = core.favorites().favoriteChannels(offset, limit)

    // ---- Recents ----

    override suspend fun recents(limit: UInt): List<Recent> = core.recents().list(limit)

    override suspend fun recentsEnabled(): Boolean = core.recents().isEnabled()

    override suspend fun setRecentsEnabled(enabled: Boolean) = core.recents().setEnabled(enabled)

    override suspend fun clearRecents() = core.recents().clear()

    override suspend fun recordRecent(channel: PlayableChannel) =
        core.recents().record(channel.sourceId, channel.identity, channel.name, channel.locator, null)

    // ---- Search ----

    override suspend fun search(
        query: String,
        sourceId: Long?,
        kind: MediaKind?,
        offset: UInt,
        limit: UInt,
    ): SearchPage = core.search().search(query, sourceId, kind, offset, limit)

    // ---- Playback ----

    /**
     * Fetches the three-row window centred on [offset] from whichever paged query [context] names.
     * One page, regardless of ring size — this is what keeps zap O(1) at 50k channels (PRD §9).
     */
    override suspend fun zapWindow(
        context: ZapContext,
        offset: UInt,
    ): ZapWindow? {
        // A window at offset 0 starts at 0 and has no previous row; elsewhere it starts one back, so
        // `current` sits in the middle.
        val start = if (offset == 0u) 0u else offset - 1u
        val limit = if (offset == 0u) 2u else 3u
        val (channels, total) = ring(context, offset = start, limit = limit)

        // `current` is the first row when the window could not step back, the second otherwise.
        val currentIndex = (offset - start).toInt()
        if (currentIndex >= channels.size) return null
        return ZapWindow(
            previous = if (currentIndex > 0) channels[currentIndex - 1] else null,
            current = channels[currentIndex],
            next = channels.getOrNull(currentIndex + 1),
            offset = offset,
            total = total,
        )
    }

    private suspend fun ring(
        context: ZapContext,
        offset: UInt,
        limit: UInt,
    ): Pair<List<PlayableChannel>, ULong?> =
        when (context) {
            ZapContext.Single -> emptyList<PlayableChannel>() to null
            is ZapContext.Group -> {
                val page =
                    core.catalog().channelsInGroup(context.sourceId, context.kind, context.group, offset, limit)
                page.channels.map(PlayableChannel::of) to page.total
            }

            ZapContext.Favorites -> {
                val page = core.favorites().favoriteChannels(offset, limit)
                page.channels.map(PlayableChannel::of) to page.total
            }

            is ZapContext.Search -> {
                val page = core.search().search(context.query, context.sourceId, context.kind, offset, limit)
                page.channels.map(PlayableChannel::of) to null
            }
        }

    override suspend fun channelEngine(
        sourceId: Long,
        identity: Long,
    ): String? = setting(PlaybackSettingKey.channelEngine(sourceId, identity))

    override suspend fun setChannelEngine(
        sourceId: Long,
        identity: Long,
        engine: String?,
    ) {
        val key = PlaybackSettingKey.channelEngine(sourceId, identity)
        if (engine != null) {
            core.settings().set(key, engine)
        } else {
            core.settings().remove(key)
        }
    }

    override suspend fun sourceEngine(sourceId: Long): String? = setting(PlaybackSettingKey.sourceEngine(sourceId))

    override suspend fun bufferingProfile(): String? = setting(PlaybackSettingKey.BUFFERING_PROFILE)

    override suspend fun setBufferingProfile(profile: String) {
        core.settings().set(PlaybackSettingKey.BUFFERING_PROFILE, profile)
    }

    private suspend fun setting(key: String): String? = core.settings().get(key)

    companion object {
        /** Opens the core against [dbPath], installing the host secrets store and log sink. */
        fun open(
            dbPath: String,
            logDirectives: String,
            secrets: SecretStore,
            logSink: LogSink,
        ): SpidolaCore {
            uniffiEnsureInitialized()
            return SpidolaCore(Core(CoreConfig(dbPath, logDirectives), secrets, logSink))
        }
    }
}
