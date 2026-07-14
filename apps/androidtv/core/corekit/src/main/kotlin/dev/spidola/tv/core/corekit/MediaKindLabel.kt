// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.core.corekit

import uniffi.core_api.MediaKind

/**
 * A couch-legible label for the "type" level of the browse drill-down and the search filter. The
 * `when` is exhaustive over the FFI enum; a new variant forces a decision here (TECH_SPEC §5).
 */
val MediaKind.label: String
    get() =
        when (this) {
            MediaKind.LIVE -> "Live"
            MediaKind.MOVIE -> "Movies"
            MediaKind.SERIES_EPISODE -> "Series"
        }
