// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.feature.settings

import androidx.compose.runtime.Composable
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import dev.spidola.tv.core.playercontract.EngineId
import uniffi.core_api.BufferingProfile
import uniffi.core_api.InterfaceDensity
import uniffi.core_api.LogLevel
import uniffi.core_api.SubtitleBackground
import uniffi.core_api.SubtitleSize

/**
 * The presentation layer for the settings vocabulary: every typed value's couch-legible name
 * (PRD §8.6), resolved from `strings.xml` so the slice is translatable (PRD §6.10).
 *
 * Labels live here rather than on the view models on purpose — a view model that holds a resolved
 * string cannot be translated and cannot be unit-tested without a `Context`. The view models carry
 * typed values; this file is the only place that turns one into words.
 *
 * Each `when` over a core enum is exhaustive with no `else`: a variant added to the boundary is a
 * compile error here until someone writes its name, which is the strongest form of the
 * "unknown future variant" arm — the build refuses to ship an unlabelled setting.
 */
@Composable
internal fun SettingValue.label(): String =
    when (this) {
        is SettingValue.DefaultEngine -> engine.engineLabel()
        is SettingValue.Buffering -> profile.label()
        is SettingValue.SubtitleGlyphSize -> size.label()
        is SettingValue.SubtitlePlate -> background.label()
        is SettingValue.Language -> choice.label()
        is SettingValue.Density -> density.label()
        is SettingValue.RecentsRetention ->
            pluralStringResource(R.plurals.settings_retention_days, days.toInt(), days.toInt())
        is SettingValue.ImageCache -> stringResource(R.string.settings_image_cache_megabytes, megabytes.toInt())
        is SettingValue.Logging -> level.label()
    }

/** The name of the setting a picker changes — the picker screen's title and its row's title. */
@Composable
internal fun SettingsPicker.title(): String =
    stringResource(
        when (this) {
            SettingsPicker.DEFAULT_ENGINE -> R.string.settings_default_player
            SettingsPicker.BUFFERING -> R.string.settings_buffering
            SettingsPicker.SUBTITLE_SIZE -> R.string.settings_subtitle_size
            SettingsPicker.SUBTITLE_BACKGROUND -> R.string.settings_subtitle_background
            SettingsPicker.LANGUAGE -> R.string.settings_language
            SettingsPicker.DENSITY -> R.string.settings_density
            SettingsPicker.RECENTS_RETENTION -> R.string.settings_retention
            SettingsPicker.IMAGE_CACHE -> R.string.settings_image_cache
            SettingsPicker.LOG_LEVEL -> R.string.diagnostics_log_level
        },
    )

/**
 * An engine's name. [EngineId] is an opaque value class rather than an enum — engine identity is
 * deliberately open so the contract never enumerates its own implementors (TECH_SPEC §8) — so this
 * `when` needs a final arm. It shows the raw key: a key this build has no name for is most likely
 * an override written by a build that shipped another engine, and showing the key a support thread
 * can quote beats inventing a name or hiding the fact that something is set.
 */
@Composable
private fun EngineId?.engineLabel(): String {
    val engine = this ?: return stringResource(R.string.settings_engine_automatic)
    return when (engine) {
        EngineId.EXOPLAYER -> stringResource(R.string.settings_engine_exoplayer)
        EngineId.MPV -> stringResource(R.string.settings_engine_mpv)
        else -> engine.value
    }
}

/**
 * The buffering trade, in the viewer's terms. The words match `player-contract`'s own
 * [dev.spidola.tv.core.playercontract.BufferingProfile] labels exactly, so this setting and the
 * in-playback options name the same choice the same way — they are spelled out again here rather
 * than read from that enum because a label baked into a contract class cannot be translated
 * (PRD §6.10).
 */
@Composable
private fun BufferingProfile.label(): String =
    stringResource(
        when (this) {
            BufferingProfile.LOW -> R.string.settings_buffering_low
            BufferingProfile.BALANCED -> R.string.settings_buffering_balanced
            BufferingProfile.GENEROUS -> R.string.settings_buffering_generous
        },
    )

@Composable
private fun SubtitleSize.label(): String =
    stringResource(
        when (this) {
            SubtitleSize.SMALL -> R.string.settings_subtitle_size_small
            SubtitleSize.MEDIUM -> R.string.settings_subtitle_size_medium
            SubtitleSize.LARGE -> R.string.settings_subtitle_size_large
        },
    )

@Composable
private fun SubtitleBackground.label(): String =
    stringResource(
        when (this) {
            SubtitleBackground.NONE -> R.string.settings_subtitle_background_none
            SubtitleBackground.SHADOW -> R.string.settings_subtitle_background_shadow
            SubtitleBackground.SOLID -> R.string.settings_subtitle_background_solid
        },
    )

@Composable
private fun LanguageChoice.label(): String =
    stringResource(
        when (this) {
            LanguageChoice.SYSTEM -> R.string.settings_language_system
            LanguageChoice.ENGLISH -> R.string.settings_language_english
        },
    )

@Composable
private fun InterfaceDensity.label(): String =
    stringResource(
        when (this) {
            InterfaceDensity.COMFORTABLE -> R.string.settings_density_comfortable
            InterfaceDensity.COMPACT -> R.string.settings_density_compact
        },
    )

/**
 * A log level's name in plain words. The levels are `tracing` levels in the core; on screen they say
 * how much Spidola writes down, because "TRACE" is jargon and the viewer choosing it is a person
 * being asked to reproduce a bug (PRD §8.6).
 */
@Composable
private fun LogLevel.label(): String =
    stringResource(
        when (this) {
            LogLevel.ERROR -> R.string.diagnostics_log_level_error
            LogLevel.WARN -> R.string.diagnostics_log_level_warn
            LogLevel.INFO -> R.string.diagnostics_log_level_info
            LogLevel.DEBUG -> R.string.diagnostics_log_level_debug
            LogLevel.TRACE -> R.string.diagnostics_log_level_trace
        },
    )
