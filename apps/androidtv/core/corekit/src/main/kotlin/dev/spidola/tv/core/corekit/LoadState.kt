// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.core.corekit

/**
 * A generic screen load-state shared by the vertical slices' view models — the shell-side
 * vocabulary for a core-backed screen. A closed set the composables match exhaustively; the
 * [Failed] arm carries a fully-formed [ActionableError] (PRD §6.3), so an error is never a bare
 * string. It lives in corekit beside [ActionableError] because every slice speaks it and features
 * never depend sideways on one another (doctrine §3.1).
 */
sealed interface LoadState<out T> {
    data object Loading : LoadState<Nothing>

    data object Empty : LoadState<Nothing>

    data class Ready<out T>(
        val value: T,
    ) : LoadState<T>

    data class Failed(
        val error: ActionableError,
    ) : LoadState<Nothing>
}
