// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv

import android.content.Context
import dev.spidola.tv.core.corekit.KeystoreSecretStore
import dev.spidola.tv.core.corekit.SpidolaCore
import dev.spidola.tv.core.corekit.SpidolaLogSink
import dev.spidola.tv.core.playercontract.EngineId
import dev.spidola.tv.core.playercontract.EngineRegistry
import dev.spidola.tv.player.engineexo.ExoEngine
import dev.spidola.tv.player.enginempv.MpvEngine

/**
 * The composition root's single wiring point for the core (TECH_SPEC §3.1: composition happens
 * only at the app shell). Manual constructor DI: the core is the one durable source of truth, so
 * it is created once here with the Keystore secrets store and logcat sink installed and nowhere
 * else. This small graph is accepted for M0; Hilt/KSP2 remains the production-hardening target as
 * the graph grows (see IMPLEMENTATION_PLAN Phase 3).
 */
class AppContainer(context: Context) {
    private val appContext: Context = context.applicationContext

    val core: SpidolaCore =
        SpidolaCore.open(
            dbPath = context.filesDir.resolve("spidola.sqlite").absolutePath,
            logDirectives = "info,spidola=debug",
            secrets = KeystoreSecretStore(context),
            logSink = SpidolaLogSink(),
        )

    /**
     * The engines this build can construct (TECH_SPEC §8): ExoPlayer the default for its platform
     * integration and hardware decoding, libmpv the fallback for its codec breadth. Engines are peers
     * wired here, never children of the playback slice (doctrine §3.1) — which is what keeps
     * `feature:playback` free of any decoder dependency. Each factory builds a fresh engine, because
     * zapping disposes and rebuilds one per channel flip.
     */
    val registry: EngineRegistry =
        EngineRegistry(
            platformDefault = EngineId.EXOPLAYER,
            factories =
                mapOf(
                    EngineId.EXOPLAYER to { ExoEngine(appContext) },
                    EngineId.MPV to { MpvEngine() },
                ),
        )

    val fixtureSeeder: FixtureSeeder = FixtureSeeder(core)
}
