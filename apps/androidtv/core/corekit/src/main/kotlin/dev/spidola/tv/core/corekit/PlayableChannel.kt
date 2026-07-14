// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.core.corekit

import uniffi.core_api.Channel
import uniffi.core_api.MediaKind
import uniffi.core_api.Recent

/**
 * The flat channel currency the shell navigates and plays with. A browse [Channel], a
 * favorite-resolved channel, and a [Recent] all map to it, so one detail/play path serves every
 * entry point (home rails, drill-down, search). It carries exactly what the shell needs to present,
 * favorite/hide, and (Phase 5) play a channel — never business state, which stays in the core.
 */
data class PlayableChannel(
    val sourceId: Long,
    /** Stable per-source identity (favorites/hidden/recents key on this), not the churny rowid. */
    val identity: Long,
    val name: String,
    val group: String?,
    val logo: String?,
    val locator: String,
    /**
     * What the source says this is. `null` when the entry point could not say — a recent snapshots
     * no kind, so nothing downstream may claim "LIVE" without evidence (PRD §8.5).
     */
    val kind: MediaKind? = null,
) {
    companion object {
        fun of(channel: Channel): PlayableChannel =
            PlayableChannel(
                sourceId = channel.sourceId,
                identity = channel.identity,
                name = channel.name,
                group = channel.groupTitle,
                logo = channel.logo,
                locator = channel.locator,
                kind = channel.kind,
            )

        /**
         * A recently-watched entry snapshots name + locator at play time, so it stays replayable
         * even if the channel later left the catalog; it carries no group, logo, or kind.
         */
        fun of(recent: Recent): PlayableChannel =
            PlayableChannel(
                sourceId = recent.sourceId,
                identity = recent.identity,
                name = recent.name,
                group = null,
                logo = null,
                locator = recent.locator,
            )
    }
}
