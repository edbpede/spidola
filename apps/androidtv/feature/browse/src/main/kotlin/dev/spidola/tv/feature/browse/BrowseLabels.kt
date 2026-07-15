// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.feature.browse

import androidx.compose.runtime.Composable
import androidx.compose.ui.res.stringResource
import uniffi.core_api.MediaKind

/**
 * The presentation layer for the browse slice's typed vocabulary, resolved from `strings.xml` so the
 * slice is translatable (PRD §6.10).
 *
 * The words duplicate corekit's [dev.spidola.tv.core.corekit.label] rather than read it, for the
 * same reason the settings slice re-spells the buffering profiles: a label baked into a Kotlin
 * property is unreachable from a translation, and corekit is not an Android resource module. The
 * `when` is exhaustive with no `else`, so a kind added to the boundary is a compile error here until
 * someone writes its name.
 */
@Composable
internal fun MediaKind.label(): String =
    stringResource(
        when (this) {
            MediaKind.LIVE -> R.string.browse_kind_live
            MediaKind.MOVIE -> R.string.browse_kind_movies
            MediaKind.SERIES_EPISODE -> R.string.browse_kind_series
        },
    )
