// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.feature.sources

import androidx.compose.runtime.Composable
import androidx.compose.ui.res.stringResource
import uniffi.core_api.Source

/**
 * The presentation layer for the sources slice's typed vocabulary, resolved from `strings.xml` so
 * the slice is translatable (PRD §6.10).
 *
 * The words live here rather than beside [Source] because naming a value is a rendering concern: the
 * boundary models what a source *is*, and what to call it belongs to whichever slice draws it. Home
 * spells the same three kinds in its own file, where a kind sits alone under a name; here it shares a
 * subtitle with a refresh schedule. The `when` is exhaustive with no `else`, so a source kind added
 * to the boundary is a compile error here until someone writes its name.
 */
@Composable
internal fun Source.kindLabel(): String =
    stringResource(
        when (this) {
            is Source.M3uUrl -> R.string.sources_kind_playlist_url
            is Source.M3uFile -> R.string.sources_kind_playlist_file
            is Source.Xtream -> R.string.sources_kind_xtream
        },
    )
