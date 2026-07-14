// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.lifecycle.lifecycleScope
import dev.spidola.tv.core.designsystem.SpidolaTheme
import kotlinx.coroutines.launch

/**
 * The single Activity (TECH_SPEC §7). It is a thin host: it installs the theme and the
 * Navigation 3 back-stack-as-state graph, handing the graph the core catalog from the app
 * container, and owns nothing else.
 */
class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val app = application as SpidolaApplication
        lifecycleScope.launch {
            app.bootstrap.await()
            setContent {
                SpidolaTheme {
                    SpidolaNavHost(core = app.container.core)
                }
            }
        }
    }
}
