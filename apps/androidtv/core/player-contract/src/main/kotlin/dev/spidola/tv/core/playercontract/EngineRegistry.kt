// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.core.playercontract

/**
 * The set of engines this build can construct, and how to construct them.
 *
 * Engines are **peers injected by the composition root**, never children of the playback feature
 * (doctrine §3.1) — so the app module owns this value and the playback slice receives it. That is
 * what keeps `feature:playback` free of any dependency on `player:engine-exo`/`player:engine-mpv`:
 * it holds engine *identities* and asks the registry to build one.
 *
 * Each factory builds a **fresh** engine. Engines are single-use by contract ([PlaybackEngine.load]
 * once, then release), because the zap path destroys and rebuilds one per channel flip.
 */
class EngineRegistry(
    /** The engine to use when no override applies — ExoPlayer on Android (TECH_SPEC §8). */
    val platformDefault: EngineId,
    private val factories: Map<EngineId, () -> PlaybackEngine>,
) {
    /** The engines actually available to the selection policy. */
    val registered: Set<EngineId> get() = factories.keys

    /** Builds a fresh engine, or `null` when [id] is not registered in this build. */
    fun make(id: EngineId): PlaybackEngine? = factories[id]?.invoke()

    /**
     * Resolves the policy (TECH_SPEC §8) against this registry and builds the winner.
     *
     * Returns `null` only when the resolved engine cannot be built — which for a correctly composed
     * app means the platform default is missing, a wiring bug the caller surfaces as one honest
     * failure rather than a silent substitution.
     */
    fun resolveAndMake(
        channelOverride: EngineId?,
        sourceOverride: EngineId?,
    ): PlaybackEngine? {
        val id =
            EngineSelection.resolve(
                channelOverride = channelOverride,
                sourceOverride = sourceOverride,
                platformDefault = platformDefault,
                registered = registered,
            )
        return make(id)
    }
}
