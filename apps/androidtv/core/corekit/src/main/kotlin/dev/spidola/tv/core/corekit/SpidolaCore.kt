// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.core.corekit

import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.callbackFlow
import uniffi.core_api.ApiException
import uniffi.core_api.ChannelPage
import uniffi.core_api.Core
import uniffi.core_api.CoreConfig
import uniffi.core_api.Handshake
import uniffi.core_api.ImportListener
import uniffi.core_api.ImportOutcome
import uniffi.core_api.ImportProgress
import uniffi.core_api.LogSink
import uniffi.core_api.SecretStore
import uniffi.core_api.Source

/**
 * Reads the source list and a source's channel catalog one page at a time (paged by contract,
 * TECH_SPEC §5). A narrow surface so a view-model can be unit-tested against a fake instead of
 * the real core.
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

/** One event from a running import; the stream terminates on [Complete] or [Failed]. */
sealed interface ImportEvent {
    data class Progress(val progress: ImportProgress) : ImportEvent

    data class Complete(val outcome: ImportOutcome) : ImportEvent

    data class Failed(val error: ApiException) : ImportEvent
}

/**
 * The single Kotlin-side handle on the Rust core (TECH_SPEC §5, §7). It wraps the generated
 * [Core], hands feature code a narrow [ChannelCatalog], and bridges the import callback interface
 * into a cold [Flow] whose cancellation reaches all the way to the core's task handle.
 */
class SpidolaCore private constructor(
    private val core: Core,
) : CatalogAccess {
    /** The startup handshake (core / schema / boundary versions), checked before first use. */
    fun handshake(): Handshake = core.handshake()

    override suspend fun sources(): List<Source> = core.sources().list()

    suspend fun addM3uUrl(
        name: String,
        url: String,
    ): Source = core.sources().addM3uUrl(name, url, null, false)

    override suspend fun page(
        sourceId: Long,
        offset: UInt,
        limit: UInt,
    ): ChannelPage = core.catalog().channels(sourceId, offset, limit)

    /**
     * Refreshes a source, emitting progress then a single terminal event. Collection — and the
     * underlying core task, cancelled at the next batch boundary — stops the instant the
     * collector's scope is cancelled (departed screen ⇒ scope ⇒ core task handle).
     */
    fun import(sourceId: Long): Flow<ImportEvent> =
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
            val handle = core.sources().refresh(sourceId, listener)
            awaitClose { handle.cancel() }
        }

    companion object {
        /** Opens the core against [dbPath], installing the host secrets store and log sink. */
        fun open(
            dbPath: String,
            logDirectives: String,
            secrets: SecretStore,
            logSink: LogSink,
        ): SpidolaCore = SpidolaCore(Core(CoreConfig(dbPath, logDirectives), secrets, logSink))
    }
}
