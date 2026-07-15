// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.feature.sources

import dev.spidola.tv.core.corekit.ImportEvent
import dev.spidola.tv.core.corekit.SourcesAccess
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import uniffi.core_api.ApiException
import uniffi.core_api.ImportOutcome
import uniffi.core_api.ImportProgress
import uniffi.core_api.ImportStage
import uniffi.core_api.Source
import uniffi.core_api.SourceCommon

/** What `addXtream` was handed, so a test can assert the credential reached the core verbatim. */
internal data class XtreamCall(
    val name: String,
    val server: String,
    val username: String,
    val password: String,
)

/**
 * A fake [SourcesAccess]: records mutations and replays a scripted import terminal event. Shared by
 * the slice's test files, so there is one description of how the core behaves rather than one per
 * suite that can drift.
 */
internal class FakeSourcesAccess(
    private val sources: List<Source> = emptyList(),
    /** Set to make the verified-before-storing add reject, the way a wrong password does. */
    private val addXtreamFailure: ApiException? = null,
    private val importResult: ImportEvent =
        ImportEvent.Complete(
            ImportOutcome(inserted = 0uL, duplicatesDropped = 0uL, emitted = 0uL, skipped = 0uL, invalid = 0uL),
        ),
) : SourcesAccess {
    var lastEnabled: Pair<Long, Boolean>? = null
        private set
    val deletedIds = mutableListOf<Long>()
    var xtreamCall: XtreamCall? = null
        private set
    private var nextId = 100L

    override suspend fun sources(): List<Source> = sources

    override suspend fun addM3uUrl(
        name: String,
        url: String,
        userAgent: String?,
        acceptInvalidTls: Boolean,
    ): Source =
        Source.M3uUrl(
            id = nextId++,
            common = SourceCommon(name = name, enabled = true, autoRefreshSecs = null),
            url = url,
            userAgent = userAgent,
            acceptInvalidTls = acceptInvalidTls,
        )

    override suspend fun addM3uFile(name: String): Source =
        Source.M3uFile(
            id = nextId++,
            common = SourceCommon(name = name, enabled = true, autoRefreshSecs = null),
        )

    /** Verifies before storing, like the core: a rejection throws and creates nothing. */
    override suspend fun addXtream(
        name: String,
        server: String,
        username: String,
        password: String,
    ): Source {
        xtreamCall = XtreamCall(name, server, username, password)
        addXtreamFailure?.let { throw it }
        return Source.Xtream(
            id = nextId++,
            common = SourceCommon(name = name, enabled = true, autoRefreshSecs = null),
            server = server,
            username = username,
            // The real core mints an opaque key, writes the password to the platform secure store
            // under it, and returns only the key — the returned Source never carries the credential.
            secretRef = "secret-$nextId",
        )
    }

    override suspend fun rename(
        id: Long,
        name: String,
    ) = Unit

    override suspend fun setEnabled(
        id: Long,
        enabled: Boolean,
    ) {
        lastEnabled = id to enabled
    }

    override suspend fun setAutoRefresh(
        id: Long,
        secs: UInt?,
    ) = Unit

    override suspend fun deleteSource(id: Long) {
        deletedIds.add(id)
    }

    override fun importUrl(id: Long): Flow<ImportEvent> = scripted()

    override fun importContent(
        id: Long,
        content: String,
    ): Flow<ImportEvent> = scripted()

    private fun scripted(): Flow<ImportEvent> =
        flow {
            emit(ImportEvent.Progress(ImportProgress(stage = ImportStage.DOWNLOADING, channelsSeen = 1uL)))
            emit(importResult)
        }
}
