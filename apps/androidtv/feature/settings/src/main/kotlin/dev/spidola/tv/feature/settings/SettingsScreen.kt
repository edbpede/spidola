// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.feature.settings

import androidx.annotation.StringRes
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.focusRestorer
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.tv.material3.MaterialTheme
import androidx.tv.material3.Text
import dev.spidola.tv.core.corekit.LoadState
import dev.spidola.tv.core.corekit.SettingsAccess
import dev.spidola.tv.core.designsystem.RowAccessory
import dev.spidola.tv.core.designsystem.SpidolaPalette
import dev.spidola.tv.core.designsystem.SpidolaRow
import dev.spidola.tv.core.designsystem.SpidolaSpacing

/**
 * The settings surface (PRD §6.9): one vertical, D-pad-traversable list under section headers.
 *
 * Every row says what the setting is, explains it in one line, and shows its current value — so the
 * list answers "what is this set to?" without entering anything. Selecting a closed-set row opens a
 * picker ([SettingsPickerScreen]); the two rows that are not choices (the recently-watched
 * off-switch, clear history) act in place. The tvOS settings shell mirrors this list unit for unit
 * (PRD §7).
 *
 * The EPG window is deliberately not offered, though the core carries it — see [SettingsSnapshot].
 */
@Composable
fun SettingsScreen(
    access: SettingsAccess,
    navigator: SettingsNavigator,
    onGoBack: () -> Unit,
    modifier: Modifier = Modifier,
    viewModel: SettingsViewModel = viewModel(factory = SettingsViewModel.factory(access)),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val status by viewModel.status.collectAsStateWithLifecycle()

    // Re-read on every entry. This view model outlives a trip to a picker, and the picker's whole
    // job is to change what this list shows — an `init`-time load alone would leave the row the
    // viewer just edited displaying its old value.
    LaunchedEffect(Unit) { viewModel.load() }

    Box(modifier = modifier.fillMaxSize().background(SpidolaPalette.Studio)) {
        when (val current = state) {
            // Settings are never empty: every setting has a default and the app is usable without
            // ever opening this screen (PRD §6.9). `Empty` is unreachable, so it shares the loading
            // arm rather than inventing an empty state that cannot occur.
            LoadState.Loading, LoadState.Empty -> Centered(stringResource(R.string.settings_loading))
            is LoadState.Failed ->
                ActionableErrorContent(current.error, onRetry = viewModel::load, onGoBack = onGoBack)
            is LoadState.Ready ->
                SettingsList(
                    snapshot = current.value,
                    status = status,
                    navigator = navigator,
                    onToggleRecents = viewModel::setRecentsEnabled,
                    onClearHistory = viewModel::clearHistory,
                )
        }
    }
}

@Composable
private fun SettingsList(
    snapshot: SettingsSnapshot,
    status: SettingsStatus?,
    navigator: SettingsNavigator,
    onToggleRecents: (Boolean) -> Unit,
    onClearHistory: () -> Unit,
) {
    // Clearing history is destructive and cannot be undone, so it asks twice — the same in-place
    // confirmation the sources list uses for delete, rather than a dialog the D-pad must escape.
    var confirmClear by remember { mutableStateOf(false) }
    val firstRow = remember { FocusRequester() }

    LazyColumn(
        modifier = Modifier.fillMaxSize().focusRestorer(firstRow),
        contentPadding =
            PaddingValues(
                horizontal = SpidolaSpacing.safeHorizontal,
                vertical = SpidolaSpacing.safeVertical,
            ),
        verticalArrangement = Arrangement.spacedBy(SpidolaSpacing.s),
    ) {
        item(key = "title") {
            Text(
                text = stringResource(R.string.settings_title),
                style = MaterialTheme.typography.displayLarge,
                color = SpidolaPalette.BroadcastWhite,
                modifier = Modifier.semantics { heading() },
            )
        }
        status?.let { current -> item(key = "status") { StatusLine(current) } }

        section(R.string.settings_section_playback)
        item(key = "default-engine") {
            ChoiceRow(
                picker = SettingsPicker.DEFAULT_ENGINE,
                snapshot = snapshot,
                explainer = R.string.settings_default_player_explainer,
                onClick = { navigator.openPicker(SettingsPicker.DEFAULT_ENGINE) },
                modifier = Modifier.focusRequester(firstRow).testTag("settings-default-engine"),
            )
        }
        item(key = "buffering") {
            ChoiceRow(
                picker = SettingsPicker.BUFFERING,
                snapshot = snapshot,
                explainer = R.string.settings_buffering_explainer,
                onClick = { navigator.openPicker(SettingsPicker.BUFFERING) },
                modifier = Modifier.testTag("settings-buffering"),
            )
        }

        section(R.string.settings_section_subtitles)
        item(key = "subtitle-size") {
            ChoiceRow(
                picker = SettingsPicker.SUBTITLE_SIZE,
                snapshot = snapshot,
                explainer = R.string.settings_subtitle_size_explainer,
                onClick = { navigator.openPicker(SettingsPicker.SUBTITLE_SIZE) },
                modifier = Modifier.testTag("settings-subtitle-size"),
            )
        }
        item(key = "subtitle-background") {
            ChoiceRow(
                picker = SettingsPicker.SUBTITLE_BACKGROUND,
                snapshot = snapshot,
                explainer = R.string.settings_subtitle_background_explainer,
                onClick = { navigator.openPicker(SettingsPicker.SUBTITLE_BACKGROUND) },
                modifier = Modifier.testTag("settings-subtitle-background"),
            )
        }

        section(R.string.settings_section_interface)
        item(key = "language") {
            ChoiceRow(
                picker = SettingsPicker.LANGUAGE,
                snapshot = snapshot,
                explainer = R.string.settings_language_explainer,
                onClick = { navigator.openPicker(SettingsPicker.LANGUAGE) },
                modifier = Modifier.testTag("settings-language"),
            )
        }
        item(key = "density") {
            ChoiceRow(
                picker = SettingsPicker.DENSITY,
                snapshot = snapshot,
                explainer = R.string.settings_density_explainer,
                onClick = { navigator.openPicker(SettingsPicker.DENSITY) },
                modifier = Modifier.testTag("settings-density"),
            )
        }

        section(R.string.settings_section_privacy)
        item(key = "recents") {
            val value = stringResource(if (snapshot.recentsEnabled) R.string.settings_on else R.string.settings_off)
            SpidolaRow(
                title = stringResource(R.string.settings_recents),
                subtitle = stringResource(R.string.settings_recents_explainer),
                accessory = RowAccessory.Label(value),
                onClick = { onToggleRecents(!snapshot.recentsEnabled) },
                modifier =
                    Modifier
                        .semantics { stateDescription = value }
                        .testTag("settings-recents"),
            )
        }
        item(key = "retention") {
            ChoiceRow(
                picker = SettingsPicker.RECENTS_RETENTION,
                snapshot = snapshot,
                explainer = R.string.settings_retention_explainer,
                onClick = { navigator.openPicker(SettingsPicker.RECENTS_RETENTION) },
                modifier = Modifier.testTag("settings-retention"),
            )
        }
        item(key = "clear-history") {
            val clearWarning = stringResource(R.string.settings_clear_history_warning)
            SpidolaRow(
                title =
                    stringResource(
                        if (confirmClear) R.string.settings_clear_history_confirm else R.string.settings_clear_history,
                    ),
                subtitle = stringResource(R.string.settings_clear_history_explainer),
                accessory = if (confirmClear) RowAccessory.Label(clearWarning) else RowAccessory.None,
                onClick = {
                    if (confirmClear) {
                        onClearHistory()
                        confirmClear = false
                    } else {
                        confirmClear = true
                    }
                },
                // The warning is the armed state, not decoration: the next press is the one that
                // cannot be taken back, and a listener has to hear that before pressing.
                modifier =
                    Modifier
                        .semantics { if (confirmClear) stateDescription = clearWarning }
                        .testTag("settings-clear-history"),
            )
        }

        section(R.string.settings_section_storage)
        item(key = "image-cache") {
            ChoiceRow(
                picker = SettingsPicker.IMAGE_CACHE,
                snapshot = snapshot,
                explainer = R.string.settings_image_cache_explainer,
                onClick = { navigator.openPicker(SettingsPicker.IMAGE_CACHE) },
                modifier = Modifier.testTag("settings-image-cache"),
            )
        }

        section(R.string.settings_section_diagnostics)
        item(key = "diagnostics") {
            SpidolaRow(
                title = stringResource(R.string.settings_diagnostics),
                subtitle = stringResource(R.string.settings_diagnostics_explainer),
                onClick = navigator.openDiagnostics,
                modifier = Modifier.testTag("settings-diagnostics"),
            )
        }
    }
}

/** A settings row that shows a setting's current value and opens its picker. */
@Composable
private fun ChoiceRow(
    picker: SettingsPicker,
    snapshot: SettingsSnapshot,
    @StringRes explainer: Int,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val value = picker.current(snapshot).label()
    SpidolaRow(
        title = picker.title(),
        subtitle = stringResource(explainer),
        accessory = RowAccessory.Label(value),
        onClick = onClick,
        // TalkBack reads the row's words already; naming the value as the row's *state* is what
        // makes it announce "Default player … ExoPlayer" as a setting rather than as three labels
        // that happen to sit together (PRD §6.10).
        modifier = modifier.semantics { stateDescription = value },
    )
}

@Composable
private fun StatusLine(status: SettingsStatus) {
    val message =
        when (status) {
            SettingsStatus.HistoryCleared -> stringResource(R.string.settings_clear_history_done)
            is SettingsStatus.Failed -> status.error.message
        }
    Text(
        text = message,
        style = MaterialTheme.typography.labelMedium,
        color = SpidolaPalette.TestCardAmber,
    )
}
