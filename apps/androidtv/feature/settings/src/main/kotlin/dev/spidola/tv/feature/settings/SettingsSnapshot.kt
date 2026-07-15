// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.feature.settings

import androidx.compose.runtime.Immutable
import dev.spidola.tv.core.playercontract.EngineId
import uniffi.core_api.AppSettings
import uniffi.core_api.BufferingProfile
import uniffi.core_api.InterfaceDensity
import uniffi.core_api.LogLevel
import uniffi.core_api.SubtitleBackground
import uniffi.core_api.SubtitleSize

/** The UI languages this build actually ships (PRD §6.10: English-first, infrastructure from day one). */
enum class LanguageChoice(
    /** The BCP-47 tag persisted by the core; `null` follows the system language. */
    val tag: String?,
) {
    SYSTEM(null),
    ENGLISH("en"),
    ;

    companion object {
        /**
         * Reads a persisted tag back. The core holds the language as an open BCP-47 string, so this
         * mapping onto a closed set has to be total; [SYSTEM] is the honest reading of a tag this
         * build has no language for, since following the system is what it would do anyway.
         */
        fun of(tag: String?): LanguageChoice = entries.firstOrNull { it.tag == tag } ?: SYSTEM
    }
}

/**
 * Every setting the app surfaces, in the shape the screens read (PRD §6.9).
 *
 * A shell-side mirror of the core's [AppSettings] rather than the generated type itself, for two
 * reasons. The generated record's properties are `var`, which makes it **unstable** to the Compose
 * compiler and would defeat skipping on every screen that reads it. And the mapping is where the
 * shell's vocabulary meets the core's: the opaque engine key becomes an [EngineId], and the raw
 * language tag becomes a [LanguageChoice].
 *
 * The EPG window is deliberately absent, though the core reports it. EPG ingest lands in Phase 8,
 * and a setting that changes nothing the viewer can observe is a UX bug rather than a feature — so
 * the shell does not carry the window until there is a guide to window (PRD §6.6).
 */
@Immutable
data class SettingsSnapshot(
    val defaultEngine: EngineId?,
    val buffering: BufferingProfile,
    val subtitleSize: SubtitleSize,
    val subtitleBackground: SubtitleBackground,
    val language: LanguageChoice,
    val density: InterfaceDensity,
    val recentsEnabled: Boolean,
    val recentsRetentionDays: UInt,
    val imageCacheMaxMb: UInt,
    val logLevel: LogLevel,
) {
    companion object {
        fun of(settings: AppSettings): SettingsSnapshot =
            SettingsSnapshot(
                defaultEngine = settings.defaultEngine?.let(::EngineId),
                buffering = settings.buffering,
                subtitleSize = settings.subtitleSize,
                subtitleBackground = settings.subtitleBackground,
                language = LanguageChoice.of(settings.language),
                density = settings.density,
                recentsEnabled = settings.recentsEnabled,
                recentsRetentionDays = settings.recentsRetentionDays,
                imageCacheMaxMb = settings.imageCacheMaxMb,
                logLevel = settings.logLevel,
            )
    }
}
