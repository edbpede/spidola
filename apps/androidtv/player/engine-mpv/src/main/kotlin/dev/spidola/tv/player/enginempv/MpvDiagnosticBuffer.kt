// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.player.enginempv

import java.util.ArrayDeque

/**
 * The last few lines mpv logged, held only long enough to classify the next failure.
 *
 * [MpvErrorMapping] needs them: mpv reports "HTTP 401", "DNS failure" and "connection
 * refused" with the same error code, and only the log text separates them.
 *
 * Bounded on purpose. An unbounded buffer of a decoder's log output on a stream that
 * reconnects all night is a slow leak on the 1 GB device PRD §9 targets, and the classifier
 * only ever reads the tail — so retaining more than the tail buys nothing and costs
 * indefinitely.
 *
 * **Entries must already be redacted** ([MpvLogRedaction]): this is a place text lingers, and
 * §12's invariant is that a credential never reaches a place it can linger in.
 *
 * Written from mpv's pump thread and read from it too, but [snapshot] may be called from a
 * caller's thread during `load()`, so access is synchronised. Contention is nil — these are
 * warn-level lines, not a firehose.
 */
internal class MpvDiagnosticBuffer(
    private val capacity: Int = DEFAULT_CAPACITY,
) {
    private val lines = ArrayDeque<String>(capacity)

    fun add(line: String) {
        val trimmed = line.trim()
        if (trimmed.isEmpty()) return
        synchronized(lines) {
            if (lines.size == capacity) lines.removeFirst()
            lines.addLast(trimmed)
        }
    }

    /** The retained lines, oldest first, as one string. Empty when mpv has said nothing. */
    fun snapshot(): String? =
        synchronized(lines) {
            if (lines.isEmpty()) null else lines.joinToString(" | ")
        }

    fun clear() {
        synchronized(lines) { lines.clear() }
    }

    private companion object {
        /**
         * Enough to catch a failure's context — FFmpeg typically emits two or three lines
         * around a failed open — without keeping a session's history.
         */
        const val DEFAULT_CAPACITY = 8
    }
}
