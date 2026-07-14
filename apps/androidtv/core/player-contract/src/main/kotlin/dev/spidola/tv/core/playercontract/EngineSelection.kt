// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.core.playercontract

/**
 * The engine selection policy (TECH_SPEC §8), identical on both platforms:
 * **per-channel override → per-source override → platform default.**
 *
 * Pure by construction — no I/O, no platform imports, no engine references — so the policy that
 * decides what a viewer watches with is unit-testable in isolation, and mirrors the Swift
 * `EngineSelection` case for case.
 */
object EngineSelection {
    /**
     * Resolves which engine should play a channel.
     *
     * [registered] is the set the composition root actually built. An override naming an engine
     * that is not registered is **ignored rather than honoured into a crash**: overrides are
     * persisted opaque strings that outlive builds, so a key can name an engine this build does not
     * link (a platform's engine list differs, or a future key round-trips through an older app).
     * Falling through to the default is the only behaviour that keeps a stale preference from
     * making a channel unplayable.
     *
     * [platformDefault] is returned even when it is not in [registered], so the caller reports one
     * honest "no engine" failure rather than this policy silently inventing a substitute.
     */
    fun resolve(
        channelOverride: EngineId?,
        sourceOverride: EngineId?,
        platformDefault: EngineId,
        registered: Set<EngineId>,
    ): EngineId {
        if (channelOverride != null && channelOverride in registered) return channelOverride
        if (sourceOverride != null && sourceOverride in registered) return sourceOverride
        return platformDefault
    }

    /**
     * The engine to offer when [current] failed with a format/decode error — the "Try other player"
     * target (TECH_SPEC §8).
     *
     * Returns `null` when there is nothing honest to offer (no other registered engine), so the UI
     * cannot present a button that would re-run the same failure. The caller must still gate on
     * [offersOtherPlayer]; this only answers "is there another engine to try".
     */
    fun alternate(
        current: EngineId,
        registered: Set<EngineId>,
    ): EngineId? =
        // Sorted so the offer is deterministic across launches: an alternate that changed between
        // two identical failures would be untestable and would make "remember for this channel"
        // remember something the viewer did not choose.
        registered
            .filterNot { it == current }
            .minByOrNull { it.value }
}
