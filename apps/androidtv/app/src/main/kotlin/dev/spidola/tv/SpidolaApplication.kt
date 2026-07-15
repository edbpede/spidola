// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv

import android.app.Application
import android.util.Log
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Deferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import uniffi.core_api.Handshake

private const val SUPPORTED_SCHEMA_VERSION = 2u
private const val SUPPORTED_BOUNDARY_VERSION = 4u

/** Refuses a core whose persisted schema or boundary this shell cannot safely interpret. */
internal fun requireCompatibleCore(handshake: Handshake) {
    check(
        handshake.coreVersion.isNotBlank() &&
            handshake.schemaVersion == SUPPORTED_SCHEMA_VERSION &&
            handshake.boundaryVersion == SUPPORTED_BOUNDARY_VERSION,
    ) {
        "Incompatible core ${handshake.coreVersion}: schema ${handshake.schemaVersion}, " +
            "boundary ${handshake.boundaryVersion}"
    }
}

/**
 * The single-Activity composition root's application object. On start it builds the app container,
 * verifies the FFI boundary handshake (fail-fast, TECH_SPEC §5) and, for the M0 walking skeleton,
 * seeds a fixture catalog through the core so the browse screen has something to render on first
 * run.
 */
class SpidolaApplication : Application() {
    lateinit var container: AppContainer
        private set

    lateinit var bootstrap: Deferred<Unit>
        private set

    private val appScope = CoroutineScope(SupervisorJob() + Dispatchers.Default)

    override fun onCreate() {
        super.onCreate()
        container = AppContainer(this)
        val handshake = container.core.handshake()
        requireCompatibleCore(handshake)
        Log.i(
            BOOT_TAG,
            "core ${handshake.coreVersion}, schema ${handshake.schemaVersion}, " +
                "boundary ${handshake.boundaryVersion}",
        )
        bootstrap = appScope.async { container.fixtureSeeder.seedIfNeeded() }
    }

    private companion object {
        const val BOOT_TAG = "spidola::boot"
    }
}
