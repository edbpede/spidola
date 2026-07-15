// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.feature.search

import androidx.compose.runtime.Composable
import androidx.compose.ui.res.stringResource
import uniffi.core_api.MediaKind

/**
 * The presentation layer for the search slice's typed vocabulary, resolved from `strings.xml` so the
 * slice is translatable (PRD §6.10).
 *
 * The words live here rather than beside [MediaKind] because naming a value is a rendering concern:
 * the boundary models what a kind *is*, and what to call it belongs to whichever slice draws it. The
 * browse slice spells the same three in its own file — here they name a filter, there they name a
 * drill-down level, and neither screen has to ask the other before rewording one. The `when` is
 * exhaustive with no `else`, so a kind added to the boundary is a compile error here until someone
 * writes its name.
 */
@Composable
internal fun MediaKind.label(): String =
    stringResource(
        when (this) {
            MediaKind.LIVE -> R.string.search_kind_live
            MediaKind.MOVIE -> R.string.search_kind_movies
            MediaKind.SERIES_EPISODE -> R.string.search_kind_series
        },
    )
