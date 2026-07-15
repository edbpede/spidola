// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.feature.browse

import androidx.compose.runtime.Composable
import androidx.compose.ui.res.stringResource
import uniffi.core_api.MediaKind
import uniffi.core_api.Source

/**
 * The presentation layer for the browse slice's typed vocabulary, resolved from `strings.xml` so the
 * slice is translatable (PRD §6.10).
 *
 * The words live here rather than beside the types they name because naming a value is a rendering
 * concern: the boundary models what a kind or a source *is*, and what to call it on screen belongs
 * to whichever slice draws it. Search spells the same three kinds in its own file and sources words
 * the same three source kinds in theirs — none of them has to agree, and a word shared between two
 * screens is a word neither can reword. Each `when` is exhaustive with no `else`, so a variant added
 * to the boundary is a compile error here until someone writes its name.
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

/** What kind of source a home row stands for, read under the name its owner gave it. */
@Composable
internal fun Source.kindLabel(): String =
    stringResource(
        when (this) {
            is Source.M3uUrl -> R.string.browse_home_source_playlist_url
            is Source.M3uFile -> R.string.browse_home_source_playlist_file
            is Source.Xtream -> R.string.browse_home_source_xtream
        },
    )
