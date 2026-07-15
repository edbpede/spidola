// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.feature.settings

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.focusRestorer
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.tv.material3.MaterialTheme
import androidx.tv.material3.Text
import dev.spidola.tv.core.corekit.SettingsAccess
import dev.spidola.tv.core.designsystem.RowAccessory
import dev.spidola.tv.core.designsystem.SpidolaPalette
import dev.spidola.tv.core.designsystem.SpidolaRow
import dev.spidola.tv.core.designsystem.SpidolaSpacing

/**
 * The option picker for one closed-set setting (PRD §6.9): the setting's name, then its values, with
 * the current one marked. Choosing writes it and returns to the settings list.
 *
 * One screen serves all nine pickers — [SettingsPicker] supplies the options and the current value,
 * and the view model writes whichever [SettingValue] comes back. Adding a closed-set setting means
 * an entry in that enum, not another screen.
 */
@Composable
fun SettingsPickerScreen(
    picker: SettingsPicker,
    access: SettingsAccess,
    onClose: () -> Unit,
    modifier: Modifier = Modifier,
    // Keyed by picker: the view-model store is keyed by type, so without this every picker would
    // share one view model and the second one opened would show the first one's setting.
    viewModel: SettingsPickerViewModel =
        viewModel(
            key = "picker-${picker.name}",
            factory = SettingsPickerViewModel.factory(access, picker),
        ),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    LaunchedEffect(Unit) { viewModel.load() }

    // The write landed; hand the viewer back to the list, which re-reads it on entry. The effect
    // restarts on every state change, so it reads the callback through `rememberUpdatedState`
    // rather than capturing whichever instance was current when it first ran.
    val closeAfterApply by rememberUpdatedState(onClose)
    LaunchedEffect(state) { if (state is PickerState.Applied) closeAfterApply() }

    Box(modifier = modifier.fillMaxSize().background(SpidolaPalette.Studio)) {
        when (val current = state) {
            PickerState.Loading, PickerState.Applied -> Centered(stringResource(R.string.settings_loading))
            is PickerState.Failed ->
                ActionableErrorContent(current.error, onRetry = viewModel::load, onGoBack = onClose)
            is PickerState.Choosing ->
                OptionList(
                    picker = picker,
                    snapshot = current.snapshot,
                    onChoose = viewModel::choose,
                )
        }
    }
}

@Composable
private fun OptionList(
    picker: SettingsPicker,
    snapshot: SettingsSnapshot,
    onChoose: (SettingValue) -> Unit,
) {
    val options = remember(picker) { picker.options() }
    val current = picker.current(snapshot)
    val selectedRow = remember { FocusRequester() }
    val selectedLabel = stringResource(R.string.settings_picker_selected)

    LazyColumn(
        modifier = Modifier.fillMaxSize().focusRestorer(selectedRow),
        contentPadding =
            PaddingValues(
                horizontal = SpidolaSpacing.safeHorizontal,
                vertical = SpidolaSpacing.safeVertical,
            ),
        verticalArrangement = Arrangement.spacedBy(SpidolaSpacing.s),
    ) {
        item(key = "title") {
            Text(
                text = picker.title(),
                style = MaterialTheme.typography.displayLarge,
                color = SpidolaPalette.BroadcastWhite,
                modifier = Modifier.padding(bottom = SpidolaSpacing.m).semantics { heading() },
            )
        }
        // A stable key per option is what lets focus restoration find the same row again: it matches
        // on composition hash, not position. Each option is a data class, so its `toString` names
        // both its arm and its value and is unique within a picker.
        itemsIndexed(options, key = { _, option -> option.toString() }) { index, option ->
            val isCurrent = option == current
            SpidolaRow(
                title = option.label(),
                accessory = if (isCurrent) RowAccessory.Label(selectedLabel) else RowAccessory.None,
                onClick = { onChoose(option) },
                modifier =
                    Modifier
                        // The proper a11y shape for a choice: TalkBack announces the marked row as
                        // selected rather than leaving "Selected" to be read as ordinary text.
                        .semantics { selected = isCurrent }
                        .testTag("option-$index")
                        // Entering the picker lands on the current value, not the top of the list —
                        // the D-pad then walks from where the viewer already is.
                        .then(if (isCurrent) Modifier.focusRequester(selectedRow) else Modifier),
            )
        }
    }
}
