// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.core.corekit

import kotlinx.coroutines.flow.Flow
import uniffi.core_api.AppSettings
import uniffi.core_api.BrowseGroupPage
import uniffi.core_api.BufferingProfile
import uniffi.core_api.ChannelNowNext
import uniffi.core_api.ChannelPage
import uniffi.core_api.CustomChannelSummary
import uniffi.core_api.CustomGroup
import uniffi.core_api.CustomImportMode
import uniffi.core_api.EpgPage
import uniffi.core_api.EpgRefreshOutcome
import uniffi.core_api.EpgRefreshProgress
import uniffi.core_api.Handshake
import uniffi.core_api.InterfaceDensity
import uniffi.core_api.LogLevel
import uniffi.core_api.MediaKind
import uniffi.core_api.NowNext
import uniffi.core_api.PairingSession
import uniffi.core_api.PairingSubmission
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

    /**
     * Adds an Xtream Codes account. The core **verifies it before storing**, so a wrong password
     * comes back as [uniffi.core_api.ApiException.Unauthorized] from this call — it belongs on the
     * add screen as a sentence, not on the next refresh as a mystery.
     *
     * [password] is in flight to the host secure store and nowhere else: hand it to this call and
     * hold it in no field, no log, and no saved state (TECH_SPEC §12). What reaches SQLite is an
     * opaque key, never the credential.
     */
    suspend fun addXtream(
        name: String,
        server: String,
        username: String,
        password: String,
    ): Source

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

/**
 * One event from a running LAN pairing session. [Started] arrives once, immediately; [Submitted]
 * arrives each time a phone posts something the core accepted.
 */
sealed interface PairingEvent {
    /** The server is up; [session] is what the pairing screen renders. */
    data class Started(
        val session: PairingSession,
    ) : PairingEvent

    /** A phone submitted a source, ready to pre-fill the add-source flow. */
    data class Submitted(
        val submission: PairingSubmission,
    ) : PairingEvent
}

/**
 * The narrow core surface the **pairing** screen needs (PRD §6.1): run a LAN server so a phone can
 * hand this TV a source, rather than making someone type a URL with a D-pad.
 *
 * Expressed as a [Flow] rather than start/stop calls because **the collector's lifetime is the
 * security model**: the server exists only while the pairing screen is on screen, so tying it to a
 * flow's collection makes "the screen went away" and "the server stopped" the same event. A shell
 * that forgets to stop it is not expressible.
 */
interface PairingAccess {
    /**
     * Runs the pairing server for as long as this flow is collected, stopping it when collection
     * ends.
     *
     * [host] is the TV's LAN address to advertise, and **the shell must supply it**: the core infers
     * it from the route out of the host, which is right on a plain LAN and wrong behind a
     * full-tunnel VPN or on a multi-homed device. `null` asks for that inference and fails loudly
     * rather than advertising an address that would not answer.
     */
    fun pair(host: String?): Flow<PairingEvent>
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

    /** Moves [channel] one slot earlier while keeping the transfer bounded to two identities. */
    suspend fun moveFavoriteBefore(
        channel: PlayableChannel,
        anchor: PlayableChannel,
    )

    /** Moves [channel] one slot later while keeping the transfer bounded to two identities. */
    suspend fun moveFavoriteAfter(
        channel: PlayableChannel,
        anchor: PlayableChannel,
    )
}

/** One event from a running guide refresh. Cancelling its flow cancels the core task. */
sealed interface EpgRefreshEvent {
    data class Progress(
        val progress: EpgRefreshProgress,
    ) : EpgRefreshEvent

    data class Complete(
        val outcome: EpgRefreshOutcome,
    ) : EpgRefreshEvent

    data class Failed(
        val error: uniffi.core_api.ApiException,
    ) : EpgRefreshEvent
}

/** The narrow guide surface used by channel details and guide configuration. */
interface EpgAccess {
    suspend fun guideSources(): List<Source>

    suspend fun epgWindowSettings(): EpgWindowSettings

    suspend fun setEpgWindow(
        aheadHours: UInt,
        behindHours: UInt,
    )

    suspend fun nowNext(
        sourceId: Long,
        channelIdentity: Long,
        nowUnix: Long,
    ): NowNext

    /** One bounded query for a catalog page; the generated core accepts at most 100 identities. */
    suspend fun nowNextBatch(
        sourceId: Long,
        channelIdentities: List<Long>,
        nowUnix: Long,
    ): List<ChannelNowNext>

    suspend fun epgWindow(
        sourceId: Long,
        channelIdentity: Long,
        earliestUnix: Long,
        latestUnix: Long,
        offset: UInt,
        limit: UInt,
    ): EpgPage

    suspend fun hasEpgFeed(sourceId: Long): Boolean

    suspend fun setXmltvFeed(
        sourceId: Long,
        url: String,
    )

    suspend fun clearXmltvFeed(sourceId: Long)

    fun refreshEpg(
        sourceId: Long,
        nowUnix: Long,
    ): Flow<EpgRefreshEvent>
}

data class EpgWindowSettings(
    val aheadHours: UInt,
    val behindHours: UInt,
)

/** A plaintext request header that exists only while a custom-channel draft is submitted. */
data class CustomRequestHeader(
    val name: String,
    val value: String,
)

/** Editable custom-channel fields. Credential-bearing values must stay out of saved state and logs. */
data class CustomChannelInput(
    val groupId: Long?,
    val name: String,
    val logo: String?,
    val locator: String,
    val userAgent: String?,
    val headers: List<CustomRequestHeader>,
)

/** The bounded custom-channel manager surface, including explicit portable sharing modes. */
interface CustomChannelsAccess {
    suspend fun customGroups(): List<CustomGroup>

    suspend fun customChannels(groupId: Long?): List<CustomChannelSummary>

    suspend fun createCustomGroup(name: String): Long

    suspend fun renameCustomGroup(
        id: Long,
        name: String,
    )

    suspend fun deleteCustomGroup(id: Long)

    suspend fun moveCustomGroupBefore(
        id: Long,
        anchorId: Long,
    )

    suspend fun moveCustomGroupAfter(
        id: Long,
        anchorId: Long,
    )

    suspend fun createCustomChannel(input: CustomChannelInput): Long

    suspend fun updateCustomChannel(
        id: Long,
        input: CustomChannelInput,
    )

    suspend fun deleteCustomChannel(id: Long)

    suspend fun moveCustomChannelBefore(
        id: Long,
        anchorId: Long,
    )

    suspend fun moveCustomChannelAfter(
        id: Long,
        anchorId: Long,
    )

    suspend fun exportCustomChannels(): String

    suspend fun importCustomChannels(
        contents: String,
        mode: CustomImportMode,
    ): ULong
}
