// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv

import android.app.SearchManager
import android.content.ContentProvider
import android.content.ContentValues
import android.content.UriMatcher
import android.database.Cursor
import android.database.MatrixCursor
import android.net.Uri
import android.util.Log
import dev.spidola.tv.core.corekit.PlayableChannel
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch
import uniffi.core_api.ApiException
import uniffi.core_api.MediaKind
import uniffi.core_api.SearchPage
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicReference

/** Android TV global-search provider backed by the core's ranked catalog search. */
class SearchSuggestionProvider : ContentProvider() {
    private val providerJob = SupervisorJob()
    private val providerScope = CoroutineScope(providerJob + Dispatchers.Default)

    override fun onCreate(): Boolean = true

    override fun query(
        uri: Uri,
        projection: Array<out String>?,
        selection: String?,
        selectionArgs: Array<out String>?,
        sortOrder: String?,
    ): Cursor {
        val cursor = MatrixCursor(SEARCH_COLUMNS)
        if (matcher.match(uri) != SEARCH_SUGGESTIONS) return cursor
        val query = selectionArgs?.firstOrNull() ?: uri.lastPathSegment.orEmpty()
        if (query.isBlank() || query == SearchManager.SUGGEST_URI_PATH_QUERY) return cursor

        val app = context?.applicationContext as? SpidolaApplication ?: return cursor
        val page =
            runBoundedQuery {
                app.bootstrap.await()
                app.container.core.search(query, null, null, 0u, SEARCH_LIMIT)
            } ?: return cursor
        page.channels.forEachIndexed { index, channel ->
            val playable = PlayableChannel.of(channel)
            app.container.tvContentPublisher.remember(playable)
            cursor.addRow(
                arrayOf<Any?>(
                    index.toLong(),
                    playable.name,
                    playable.group.orEmpty(),
                    playable.logo,
                    "video/*",
                    if (playable.kind == MediaKind.LIVE) 1 else 0,
                    channelDeepLink(playable),
                ),
            )
        }
        return cursor
    }

    override fun shutdown() {
        providerScope.cancel()
        super.shutdown()
    }

    private fun runBoundedQuery(query: suspend () -> SearchPage): SearchPage? {
        val outcome = AtomicReference<Result<SearchPage>?>()
        val finished = CountDownLatch(1)
        val job: Job =
            providerScope.launch {
                try {
                    outcome.set(Result.success(query()))
                } catch (error: CancellationException) {
                    outcome.set(Result.failure(error))
                    throw error
                } catch (error: ApiException) {
                    outcome.set(Result.failure(error))
                } catch (error: IllegalStateException) {
                    outcome.set(Result.failure(error))
                } finally {
                    finished.countDown()
                }
            }
        if (!finished.await(QUERY_TIMEOUT_MILLIS, TimeUnit.MILLISECONDS)) {
            job.cancel()
            Log.w(TAG, "system search query timed out")
            return null
        }
        return outcome.get()?.getOrElse { error ->
            Log.w(TAG, "system search query failed", error)
            null
        }
    }

    override fun getType(uri: Uri): String = SearchManager.SUGGEST_MIME_TYPE

    override fun insert(
        uri: Uri,
        values: ContentValues?,
    ): Uri? = null

    override fun delete(
        uri: Uri,
        selection: String?,
        selectionArgs: Array<out String>?,
    ): Int = 0

    override fun update(
        uri: Uri,
        values: ContentValues?,
        selection: String?,
        selectionArgs: Array<out String>?,
    ): Int = 0

    private companion object {
        const val AUTHORITY = "dev.spidola.tv.search"
        const val SEARCH_SUGGESTIONS = 1
        const val SEARCH_LIMIT = 24u
        const val QUERY_TIMEOUT_MILLIS = 2_000L
        const val TAG = "spidola::search"

        val SEARCH_COLUMNS =
            arrayOf(
                "_id",
                SearchManager.SUGGEST_COLUMN_TEXT_1,
                SearchManager.SUGGEST_COLUMN_TEXT_2,
                SearchManager.SUGGEST_COLUMN_RESULT_CARD_IMAGE,
                SearchManager.SUGGEST_COLUMN_CONTENT_TYPE,
                SearchManager.SUGGEST_COLUMN_IS_LIVE,
                SearchManager.SUGGEST_COLUMN_INTENT_DATA,
            )

        val matcher =
            UriMatcher(UriMatcher.NO_MATCH).apply {
                addURI(AUTHORITY, SearchManager.SUGGEST_URI_PATH_QUERY, SEARCH_SUGGESTIONS)
                addURI(AUTHORITY, "${SearchManager.SUGGEST_URI_PATH_QUERY}/*", SEARCH_SUGGESTIONS)
            }
    }
}
