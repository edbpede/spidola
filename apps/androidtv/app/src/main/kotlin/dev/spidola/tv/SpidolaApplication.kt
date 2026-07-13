// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv

import android.app.Application
import android.util.Log
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch

/**
 * The single-Activity composition root's application object. On start it builds the app container,
 * verifies the FFI boundary handshake (fail-fast, TECH_SPEC §5) and, for the M0 walking skeleton,
 * seeds a fixture catalog through the core so the browse screen has something to render on first
 * run.
 */
class SpidolaApplication : Application() {
    lateinit var container: AppContainer
        private set

    private val appScope = CoroutineScope(SupervisorJob() + Dispatchers.Default)

    override fun onCreate() {
        super.onCreate()
        container = AppContainer(this)
        val handshake = container.core.handshake()
        Log.i(
            BOOT_TAG,
            "core ${handshake.coreVersion}, schema ${handshake.schemaVersion}, " +
                "boundary ${handshake.boundaryVersion}",
        )
        appScope.launch { container.fixtureSeeder.seedIfNeeded() }
    }

    private companion object {
        const val BOOT_TAG = "spidola::boot"
    }
}
