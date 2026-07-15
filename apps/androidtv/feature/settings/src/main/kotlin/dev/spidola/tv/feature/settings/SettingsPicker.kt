// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.feature.settings

import androidx.compose.runtime.Immutable
import dev.spidola.tv.core.playercontract.EngineId
import uniffi.core_api.BufferingProfile
import uniffi.core_api.InterfaceDensity
import uniffi.core_api.LogLevel
import uniffi.core_api.SubtitleBackground
import uniffi.core_api.SubtitleSize

/**
 * One chosen value for one setting.
 *
 * The picker screen hands this back to the view model, which writes it with an exhaustive `when`.
 * Typed rather than a string id precisely so that round trip cannot go wrong: there is no
 * `valueOf` to throw, no id to typo, and adding a setting is a compile error until it is written.
 */
@Immutable
sealed interface SettingValue {
    /** `null` engine means Automatic — the platform default resolves it (TECH_SPEC §8). */
    data class DefaultEngine(
        val engine: EngineId?,
    ) : SettingValue

    data class Buffering(
        val profile: BufferingProfile,
    ) : SettingValue

    data class SubtitleGlyphSize(
        val size: SubtitleSize,
    ) : SettingValue

    data class SubtitlePlate(
        val background: SubtitleBackground,
    ) : SettingValue

    data class Language(
        val choice: LanguageChoice,
    ) : SettingValue

    data class Density(
        val density: InterfaceDensity,
    ) : SettingValue

    data class RecentsRetention(
        val days: UInt,
    ) : SettingValue

    data class ImageCache(
        val megabytes: UInt,
    ) : SettingValue

    data class Logging(
        val level: LogLevel,
    ) : SettingValue
}

/**
 * The engines a viewer may choose as the default, in offer order. `null` leads: Automatic is the
 * default and the row a viewer who never opens settings never has to think about (PRD §6.9).
 *
 * The keys come from player-contract, which owns engine identity — the settings slice names the
 * engines it offers but invents no key of its own (TECH_SPEC §8).
 */
internal val OFFERED_ENGINES: List<EngineId?> = listOf(null, EngineId.EXOPLAYER, EngineId.MPV)

/** The recently-watched retention periods offered, in days. */
internal val OFFERED_RETENTION_DAYS: List<UInt> = listOf(30u, 90u, 365u)

/** The image-cache ceilings offered, in megabytes. */
internal val OFFERED_IMAGE_CACHE_MB: List<UInt> = listOf(128u, 256u, 512u)

/**
 * The closed set of settings that open a picker screen — one screen, nine settings (PRD §6.9). A
 * setting is here when its values are a closed set the viewer chooses from; the recents off-switch
 * and clear-history are actions on the settings list itself, not pickers.
 */
enum class SettingsPicker {
    DEFAULT_ENGINE,
    BUFFERING,
    SUBTITLE_SIZE,
    SUBTITLE_BACKGROUND,
    LANGUAGE,
    DENSITY,
    RECENTS_RETENTION,
    IMAGE_CACHE,
    LOG_LEVEL,
    ;

    /** Every value this setting offers, in the order the picker lists them. */
    fun options(): List<SettingValue> =
        when (this) {
            DEFAULT_ENGINE -> OFFERED_ENGINES.map(SettingValue::DefaultEngine)
            BUFFERING -> BufferingProfile.entries.map(SettingValue::Buffering)
            SUBTITLE_SIZE -> SubtitleSize.entries.map(SettingValue::SubtitleGlyphSize)
            SUBTITLE_BACKGROUND -> SubtitleBackground.entries.map(SettingValue::SubtitlePlate)
            LANGUAGE -> LanguageChoice.entries.map(SettingValue::Language)
            DENSITY -> InterfaceDensity.entries.map(SettingValue::Density)
            RECENTS_RETENTION -> OFFERED_RETENTION_DAYS.map(SettingValue::RecentsRetention)
            IMAGE_CACHE -> OFFERED_IMAGE_CACHE_MB.map(SettingValue::ImageCache)
            LOG_LEVEL -> LogLevel.entries.map(SettingValue::Logging)
        }

    /**
     * This setting's current value, so the picker can mark it and the settings list can show it.
     *
     * A value the core holds that is not among [options] — a retention period from a build that
     * offered more — still reports honestly here, because the value is read from the snapshot rather
     * than matched against the offer list. The picker then marks nothing, which is the truth.
     */
    fun current(snapshot: SettingsSnapshot): SettingValue =
        when (this) {
            DEFAULT_ENGINE -> SettingValue.DefaultEngine(snapshot.defaultEngine)
            BUFFERING -> SettingValue.Buffering(snapshot.buffering)
            SUBTITLE_SIZE -> SettingValue.SubtitleGlyphSize(snapshot.subtitleSize)
            SUBTITLE_BACKGROUND -> SettingValue.SubtitlePlate(snapshot.subtitleBackground)
            LANGUAGE -> SettingValue.Language(snapshot.language)
            DENSITY -> SettingValue.Density(snapshot.density)
            RECENTS_RETENTION -> SettingValue.RecentsRetention(snapshot.recentsRetentionDays)
            IMAGE_CACHE -> SettingValue.ImageCache(snapshot.imageCacheMaxMb)
            LOG_LEVEL -> SettingValue.Logging(snapshot.logLevel)
        }
}
