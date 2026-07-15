// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.feature.sources

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.getAndUpdate
import uniffi.core_api.PairingSubmission

/**
 * Carries a pairing submission from the pairing screen to the add-source form, in memory only.
 *
 * This exists because the obvious route — a payload on the Navigation 3 key — is unsafe for this
 * particular cargo. The back stack is serialized into saved instance state, so an Xtream
 * submission's password would be written to disk by the framework, and "hand it to `addXtream` and
 * hold it nowhere else" (TECH_SPEC §12) would be quietly untrue. A single in-memory slot dies with
 * the process, which is the lifetime a credential in flight should have.
 *
 * [take] empties the slot, so a submission pre-fills the form exactly once: re-entering add-source
 * later gets a blank form rather than someone else's account.
 */
class PairingHandoff {
    private val pending = MutableStateFlow<AddSourcePrefill?>(null)

    /** Offers what a phone submitted. Overwrites any unclaimed one — the newest submission is the
     * one whoever is standing at the TV just sent. */
    fun offer(submission: PairingSubmission) {
        pending.value = submission.toPrefill()
    }

    /** Claims the pending prefill, if any, and empties the slot. */
    fun take(): AddSourcePrefill? = pending.getAndUpdate { null }
}

/**
 * Flattens a submission into the form's shape. The `when` is exhaustive over the boundary's sealed
 * type, so a submission kind added to the core forces a decision here (TECH_SPEC §5).
 */
private fun PairingSubmission.toPrefill(): AddSourcePrefill =
    when (this) {
        is PairingSubmission.M3uUrl -> AddSourcePrefill(mode = AddSourceMode.URL, url = url)
        is PairingSubmission.Xtream ->
            AddSourcePrefill(
                mode = AddSourceMode.XTREAM,
                server = server,
                username = username,
                password = password,
            )
    }
