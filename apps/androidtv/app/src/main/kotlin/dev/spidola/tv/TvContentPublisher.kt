// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv

import android.content.ContentUris
import android.content.ContentValues
import android.content.Context
import android.content.Intent
import android.media.tv.TvContract
import android.net.Uri
import android.util.Log
import dev.spidola.tv.core.corekit.HomeAccess
import dev.spidola.tv.core.corekit.PlayableChannel
import kotlinx.coroutines.CancellationException
import uniffi.core_api.ApiException
import uniffi.core_api.MediaKind
import java.util.concurrent.ConcurrentHashMap

/** Publishes favorites and recents through Android TV's API-26 TvProvider contracts. */
class TvContentPublisher(
    context: Context,
) {
    private val appContext = context.applicationContext
    private val resolver = appContext.contentResolver
    private val preferences = appContext.getSharedPreferences(PREFERENCES, Context.MODE_PRIVATE)
    private val channels = ConcurrentHashMap<String, PlayableChannel>()

    fun remember(channel: PlayableChannel) {
        channels[channel.contentKey()] = channel
    }

    fun channel(
        sourceId: Long,
        identity: Long,
    ): PlayableChannel? = channels["$sourceId:$identity"]

    suspend fun sync(access: HomeAccess) {
        try {
            val favorites = access.favoriteChannels(0u, HOME_LIMIT).channels.map(PlayableChannel::of)
            replacePreviewPrograms(ensureHomeChannel(), favorites)
            val recents = if (access.recentsEnabled()) access.recents(WATCH_NEXT_LIMIT) else emptyList()
            replaceWatchNext(recents.map(PlayableChannel::of))
        } catch (error: CancellationException) {
            throw error
        } catch (error: ApiException) {
            Log.w(TAG, "TV content sync failed", error)
        } catch (error: SecurityException) {
            Log.w(TAG, "TV content sync was denied", error)
        } catch (error: IllegalStateException) {
            Log.w(TAG, "TV content sync failed", error)
        }
    }

    fun publishWatchNext(channel: PlayableChannel) {
        runCatching {
            val key = channel.contentKey()
            if (key in preferences.getStringSet(SUPPRESSED_WATCH_NEXT, emptySet()).orEmpty()) return
            val entries = storedEntries(WATCH_NEXT_ROWS).toMutableMap()
            entries.remove(key)?.let(::deleteWatchNext)
            val rowId = insertWatchNext(channel) ?: return
            entries[key] = rowId
            storeEntries(WATCH_NEXT_ROWS, entries)
        }.onFailure { error -> Log.w(TAG, "Watch Next publish failed", error) }
    }

    fun onPreviewProgramDisabled(rowId: Long) {
        suppressRow(PREVIEW_ROWS, SUPPRESSED_PREVIEW, rowId)
    }

    fun onWatchNextProgramDisabled(rowId: Long) {
        suppressRow(WATCH_NEXT_ROWS, SUPPRESSED_WATCH_NEXT, rowId)
    }

    private fun ensureHomeChannel(): Long {
        val stored = preferences.getLong(HOME_CHANNEL, NO_ROW)
        if (stored != NO_ROW) return stored
        val values =
            ContentValues().apply {
                put(TvContract.Channels.COLUMN_TYPE, TvContract.Channels.TYPE_PREVIEW)
                put(TvContract.Channels.COLUMN_DISPLAY_NAME, appContext.getString(R.string.home_channel_name))
                put(TvContract.Channels.COLUMN_DESCRIPTION, appContext.getString(R.string.home_channel_description))
                put(TvContract.Channels.COLUMN_APP_LINK_INTENT_URI, homeDeepLink())
                put(TvContract.Channels.COLUMN_INTERNAL_PROVIDER_ID, HOME_PROVIDER_ID)
            }
        val uri = checkNotNull(resolver.insert(TvContract.Channels.CONTENT_URI, values))
        val channelId = ContentUris.parseId(uri)
        preferences.edit().putLong(HOME_CHANNEL, channelId).apply()
        TvContract.requestChannelBrowsable(appContext, channelId)
        return channelId
    }

    private fun replacePreviewPrograms(
        channelId: Long,
        channels: List<PlayableChannel>,
    ) {
        storedEntries(PREVIEW_ROWS).values.forEach(::deletePreview)
        val suppressed = preferences.getStringSet(SUPPRESSED_PREVIEW, emptySet()).orEmpty()
        val entries =
            channels
                .filterNot { it.contentKey() in suppressed }
                .mapIndexedNotNull { index, channel ->
                    insertPreview(channelId, channel, channels.size - index)?.let { channel.contentKey() to it }
                }.toMap()
        storeEntries(PREVIEW_ROWS, entries)
    }

    private fun replaceWatchNext(channels: List<PlayableChannel>) {
        storedEntries(WATCH_NEXT_ROWS).values.forEach(::deleteWatchNext)
        val suppressed = preferences.getStringSet(SUPPRESSED_WATCH_NEXT, emptySet()).orEmpty()
        val entries =
            channels
                .filterNot { it.contentKey() in suppressed }
                .mapNotNull { channel -> insertWatchNext(channel)?.let { channel.contentKey() to it } }
                .toMap()
        storeEntries(WATCH_NEXT_ROWS, entries)
    }

    private fun insertPreview(
        channelId: Long,
        channel: PlayableChannel,
        weight: Int,
    ): Long? {
        val values =
            baseProgramValues(channel).apply {
                put(TvContract.PreviewPrograms.COLUMN_CHANNEL_ID, channelId)
                put(TvContract.PreviewPrograms.COLUMN_WEIGHT, weight)
            }
        return resolver.insert(TvContract.PreviewPrograms.CONTENT_URI, values)?.let(ContentUris::parseId)
    }

    private fun insertWatchNext(channel: PlayableChannel): Long? {
        val values =
            baseProgramValues(channel).apply {
                put(
                    TvContract.WatchNextPrograms.COLUMN_WATCH_NEXT_TYPE,
                    TvContract.WatchNextPrograms.WATCH_NEXT_TYPE_CONTINUE,
                )
                put(TvContract.WatchNextPrograms.COLUMN_LAST_ENGAGEMENT_TIME_UTC_MILLIS, System.currentTimeMillis())
            }
        return resolver.insert(TvContract.WatchNextPrograms.CONTENT_URI, values)?.let(ContentUris::parseId)
    }

    private fun baseProgramValues(channel: PlayableChannel): ContentValues =
        ContentValues().apply {
            remember(channel)
            put(TvContract.PreviewPrograms.COLUMN_TYPE, TvContract.PreviewPrograms.TYPE_CHANNEL)
            put(TvContract.PreviewPrograms.COLUMN_TITLE, channel.name)
            put(TvContract.PreviewPrograms.COLUMN_CONTENT_ID, channel.contentKey())
            put(TvContract.PreviewPrograms.COLUMN_INTERNAL_PROVIDER_ID, channel.contentKey())
            put(TvContract.PreviewPrograms.COLUMN_INTENT_URI, channelDeepLink(channel))
            put(TvContract.PreviewPrograms.COLUMN_LIVE, if (channel.kind == MediaKind.LIVE) 1 else 0)
            channel.logo?.takeIf(String::isNotBlank)?.let { put(TvContract.PreviewPrograms.COLUMN_POSTER_ART_URI, it) }
        }

    private fun suppressRow(
        rowsKey: String,
        suppressedKey: String,
        rowId: Long,
    ) {
        val entries = storedEntries(rowsKey).toMutableMap()
        val contentKey = entries.entries.firstOrNull { it.value == rowId }?.key ?: return
        entries.remove(contentKey)
        val suppressed = preferences.getStringSet(suppressedKey, emptySet()).orEmpty() + contentKey
        preferences.edit().putStringSet(suppressedKey, suppressed).apply()
        storeEntries(rowsKey, entries)
    }

    private fun storedEntries(key: String): Map<String, Long> =
        preferences.getStringSet(key, emptySet()).orEmpty().mapNotNull(::decodeStoredEntry).toMap()

    private fun storeEntries(
        key: String,
        entries: Map<String, Long>,
    ) {
        preferences
            .edit()
            .putStringSet(key, entries.map { (content, row) -> "$content$ENTRY_SEPARATOR$row" }.toSet())
            .apply()
    }

    private fun deletePreview(rowId: Long) {
        resolver.delete(TvContract.buildPreviewProgramUri(rowId), null, null)
    }

    private fun deleteWatchNext(rowId: Long) {
        resolver.delete(TvContract.buildWatchNextProgramUri(rowId), null, null)
    }

    private companion object {
        const val TAG = "spidola::tv"
        const val PREFERENCES = "tv-content"
        const val HOME_PROVIDER_ID = "spidola-favorites"
        const val HOME_CHANNEL = "home-channel"
        const val PREVIEW_ROWS = "preview-rows"
        const val WATCH_NEXT_ROWS = "watch-next-rows"
        const val SUPPRESSED_PREVIEW = "suppressed-preview"
        const val SUPPRESSED_WATCH_NEXT = "suppressed-watch-next"
        const val ENTRY_SEPARATOR = '|'
        const val NO_ROW = -1L
        const val HOME_LIMIT = 40u
        const val WATCH_NEXT_LIMIT = 20u
    }
}

internal fun channelDeepLink(channel: PlayableChannel): String =
    "spidola://channel?sourceId=${channel.sourceId}&identity=${channel.identity}"

private fun homeDeepLink(): String =
    Intent(Intent.ACTION_VIEW, Uri.parse("spidola://home"))
        .toUri(Intent.URI_INTENT_SCHEME)

internal fun PlayableChannel.contentKey(): String = "$sourceId:$identity"

internal fun decodeStoredEntry(value: String): Pair<String, Long>? {
    val separator = value.lastIndexOf('|')
    if (separator <= 0) return null
    val rowId = value.substring(separator + 1).toLongOrNull() ?: return null
    return value.substring(0, separator) to rowId
}
