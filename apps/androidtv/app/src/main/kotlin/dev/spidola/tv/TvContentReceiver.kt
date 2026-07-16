// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.media.tv.TvContract
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch

/** Handles launcher requests to rebuild or remove Spidola-owned TV provider rows. */
class TvContentReceiver : BroadcastReceiver() {
    override fun onReceive(
        context: Context,
        intent: Intent,
    ) {
        val pending = goAsync()
        CoroutineScope(SupervisorJob() + Dispatchers.IO).launch {
            try {
                val app = context.applicationContext as SpidolaApplication
                val publisher = app.container.tvContentPublisher
                when (intent.action) {
                    TvContract.ACTION_INITIALIZE_PROGRAMS -> {
                        app.bootstrap.await()
                        publisher.sync(app.container.core)
                    }
                    TvContract.ACTION_PREVIEW_PROGRAM_BROWSABLE_DISABLED ->
                        publisher.onPreviewProgramDisabled(
                            intent.getLongExtra(TvContract.EXTRA_PREVIEW_PROGRAM_ID, NO_ROW),
                        )
                    TvContract.ACTION_WATCH_NEXT_PROGRAM_BROWSABLE_DISABLED ->
                        publisher.onWatchNextProgramDisabled(
                            intent.getLongExtra(TvContract.EXTRA_WATCH_NEXT_PROGRAM_ID, NO_ROW),
                        )
                }
            } finally {
                pending.finish()
            }
        }
    }

    private companion object {
        const val NO_ROW = -1L
    }
}
