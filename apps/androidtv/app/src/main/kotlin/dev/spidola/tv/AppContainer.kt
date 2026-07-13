// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv

import android.content.Context
import dev.spidola.tv.core.corekit.CatalogAccess
import dev.spidola.tv.core.corekit.KeystoreSecretStore
import dev.spidola.tv.core.corekit.SpidolaCore
import dev.spidola.tv.core.corekit.SpidolaLogSink

/**
 * The composition root's single wiring point for the core (TECH_SPEC §3.1: composition happens
 * only at the app shell). Manual constructor DI: the core is the one durable source of truth, so
 * it is created once here with the Keystore secrets store and logcat sink installed and nowhere
 * else. (Hilt is deferred until a Dagger release supports the pinned Kotlin's class metadata; see
 * IMPLEMENTATION_PLAN Phase 3.)
 */
class AppContainer(context: Context) {
    val core: SpidolaCore =
        SpidolaCore.open(
            dbPath = context.filesDir.resolve("spidola.sqlite").absolutePath,
            logDirectives = "info,spidola=debug",
            secrets = KeystoreSecretStore(context),
            logSink = SpidolaLogSink(),
        )

    val catalog: CatalogAccess get() = core

    val fixtureSeeder: FixtureSeeder = FixtureSeeder(core)
}
