// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.player.engineexo

import dev.spidola.tv.core.playercontract.StreamRequest
import java.net.URI

/**
 * The logcat tag for playback, matching the core's `playback` span target so one
 * `logcat -s spidola::playback` shows engine and core together (TECH_SPEC §4.8).
 */
internal const val PLAYBACK_TAG = "spidola::playback"

/**
 * What this engine is allowed to say about a request.
 *
 * TECH_SPEC §12 makes "secrets never enter log messages" a hard invariant, and a [StreamRequest] is
 * secret-bearing in three places at once: header values carry auth tokens, the user-agent can be a
 * token in disguise, and — the one that is easy to miss — the locator itself does, because an
 * Xtream stream URL embeds the subscriber's username and password directly in its path
 * (`http://host/live/<user>/<pass>/123.ts`).
 *
 * So the summary reports header *names*, whether a user-agent override exists, and a locator cut
 * back to its origin. Everything a support thread needs to reconstruct the shape of a request;
 * nothing that can authenticate as the viewer.
 */
internal fun StreamRequest.logSummary(): String {
    val headerNames = headers.joinToString(separator = ",") { it.name }.ifEmpty { "none" }
    val agent = if (userAgent == null) "default" else "override"
    return "${redactLocator(locator)} buffering=$buffering headers=[$headerNames] userAgent=$agent"
}

/**
 * Reduces a locator to `scheme://host:port/…`, dropping the path, query, and any `user:pass@`
 * userinfo. An unparsable locator degrades to a marker rather than falling back to the raw string,
 * because the fallback is exactly the case most likely to be malformed *and* credential-bearing.
 */
internal fun redactLocator(locator: String): String {
    val uri = runCatching { URI(locator) }.getOrNull()
    val host = uri?.host
    return when {
        uri == null -> "<unparsable>"
        host == null -> "<opaque>"
        else -> {
            val port = if (uri.port >= 0) ":${uri.port}" else ""
            "${uri.scheme ?: "?"}://$host$port/…"
        }
    }
}
