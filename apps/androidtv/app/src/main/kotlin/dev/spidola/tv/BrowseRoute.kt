// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv

import androidx.navigation3.runtime.NavKey
import kotlinx.serialization.Serializable

/** The browse destination — the only screen in the M0 skeleton; more routes land in later phases. */
@Serializable
data object BrowseRoute : NavKey
