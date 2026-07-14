// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.player.enginempv

/**
 * Strips credential-shaped text out of mpv's own log output before it can reach logcat
 * (TECH_SPEC §4.8, §12).
 *
 * This exists because mpv and FFmpeg log the URL they are working on, and an IPTV URL is
 * frequently *made of* credentials. Three shapes all carry secrets and all appear in real
 * mpv output:
 *
 *  - userinfo:  `http://user:pass@host/stream.ts`
 *  - query:     `http://host/live?username=u&password=p`
 *  - **path**:  `http://host:8080/live/USER/PASS/123.ts` — the Xtream convention, and the
 *    one a naive redactor misses, because nothing about `/live/u/p/1.ts` looks like a
 *    secret to a pattern matcher.
 *
 * So the rule is not "find the secrets and hide them" — that is a blocklist, and the path
 * case proves a blocklist cannot be complete here. The rule is the inverse: **keep only the
 * scheme and host, discard the rest of every URL unconditionally.** Host reachability is
 * the whole diagnostic value of these lines for a support thread ("it couldn't reach
 * example.com"); the path never is.
 */
internal object MpvLogRedaction {
    /** What replaces every discarded URL component. Deliberately conspicuous in a log. */
    private const val REDACTED = "<redacted>"

    // Scheme per RFC 3986, then everything up to whitespace or a quote. Kept deliberately
    // greedy on the tail: over-matching costs a truncated log line, under-matching leaks.
    private val URL = Regex("""\b([a-zA-Z][a-zA-Z0-9+.\-]*)://([^\s'"<>]+)""")

    /**
     * Option/property assignments whose value is a secret by definition. mpv echoes these
     * back when a level above `warn` is enabled, and the engine sets both of them
     * ([MpvEngine] maps `StreamRequest.headers` and `userAgent` onto them).
     *
     * The tail is `.*` to end of line, not `\S+`. A header value contains spaces —
     * `http-header-fields=Authorization: Bearer <token>` is the shape that actually occurs —
     * so stopping at the first whitespace redacts the word `Authorization:` and leaves the
     * token itself in the log. Once a secret-bearing key is seen, the remainder of the line
     * is its value and all of it goes.
     */
    private val SECRET_ASSIGNMENT =
        Regex(
            """\b(http-header-fields|user-agent|http-proxy|Authorization|Cookie)\s*[=:].*$""",
            setOf(RegexOption.IGNORE_CASE, RegexOption.MULTILINE),
        )

    /**
     * Returns [line] with every URL reduced to `scheme://host[:port]/<redacted>` and every
     * secret-by-definition assignment reduced to its name.
     *
     * Pure and total: any input string is safe to pass, and the result never contains more
     * information than the input did.
     */
    fun redact(line: String): String = redactAssignments(redactUrls(line))

    private fun redactUrls(line: String): String =
        URL.replace(line) { match ->
            val scheme = match.groupValues[1]
            val rest = match.groupValues[2]

            // Authority ends at the first '/', '?' or '#'. Slicing here — before looking for
            // userinfo — is what makes a later '@' in the path (which is legal) unable to be
            // mistaken for a userinfo delimiter.
            val authority = rest.takeWhile { it != '/' && it != '?' && it != '#' }
            // userinfo cannot contain an unescaped '@', so the last '@' in the authority is
            // the real delimiter.
            val hostPort = authority.substringAfterLast('@')
            val hadMore = rest.length > authority.length

            when {
                hostPort.isEmpty() -> "$scheme://$REDACTED"
                hadMore -> "$scheme://$hostPort/$REDACTED"
                else -> "$scheme://$hostPort"
            }
        }

    private fun redactAssignments(line: String): String =
        SECRET_ASSIGNMENT.replace(line) { match ->
            "${match.groupValues[1]}=$REDACTED"
        }
}
