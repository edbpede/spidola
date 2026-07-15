// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.feature.playback

import androidx.compose.runtime.Composable
import androidx.compose.ui.res.stringResource
import dev.spidola.tv.core.playercontract.AspectMode

/**
 * The presentation layer for the playback slice's typed vocabulary, resolved from `strings.xml` so
 * the slice is translatable (PRD §6.10).
 *
 * The words live here rather than beside [AspectMode] because naming a value is a rendering concern:
 * the contract models what a mode *is* and what it does to a picture, and what to call it belongs to
 * whichever slice puts it on screen — the same reason the settings slice re-spells the buffering
 * profiles instead of reading them off the enum. The `when` is exhaustive with no `else`, so a mode
 * added to the contract is a compile error here until someone writes its name.
 */
@Composable
internal fun AspectMode.label(): String =
    stringResource(
        when (this) {
            AspectMode.FIT -> R.string.playback_aspect_fit
            AspectMode.FILL -> R.string.playback_aspect_fill
            AspectMode.STRETCH -> R.string.playback_aspect_stretch
        },
    )
