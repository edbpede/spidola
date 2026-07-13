// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.core.corekit

import android.util.Log
import uniffi.core_api.LogLevel
import uniffi.core_api.LogRecord
import uniffi.core_api.LogSink

/**
 * Drains the core's `tracing` pipeline into logcat (TECH_SPEC §4.8). Each core span target
 * becomes the logcat tag, so a single `logcat -s spidola::import` (etc.) shows the whole
 * pipeline; shell code logs under the same tag scheme. Levels map one-to-one.
 *
 * Secrets are provably absent: the core's secret types redact Debug and never format raw, so
 * no credential-shaped value can reach a [LogRecord] — this sink only forwards what it is given.
 *
 * UniFFI may invoke [log] on any core thread; `android.util.Log` is thread-safe.
 */
class SpidolaLogSink : LogSink {
    override fun log(record: LogRecord) {
        val tag = record.target
        when (record.level) {
            LogLevel.ERROR -> Log.e(tag, record.message)
            LogLevel.WARN -> Log.w(tag, record.message)
            LogLevel.INFO -> Log.i(tag, record.message)
            LogLevel.DEBUG -> Log.d(tag, record.message)
            LogLevel.TRACE -> Log.v(tag, record.message)
        }
    }
}
