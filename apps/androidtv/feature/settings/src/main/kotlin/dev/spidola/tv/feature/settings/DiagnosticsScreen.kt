// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.feature.settings

import androidx.annotation.StringRes
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.focusable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsFocusedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.focusRestorer
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.tv.material3.MaterialTheme
import androidx.tv.material3.Text
import dev.spidola.tv.core.corekit.LoadState
import dev.spidola.tv.core.corekit.SettingsAccess
import dev.spidola.tv.core.designsystem.RowAccessory
import dev.spidola.tv.core.designsystem.SpidolaFocus
import dev.spidola.tv.core.designsystem.SpidolaPalette
import dev.spidola.tv.core.designsystem.SpidolaRow
import dev.spidola.tv.core.designsystem.SpidolaSpacing
import kotlinx.collections.immutable.ImmutableList
import uniffi.core_api.LogLevel

/**
 * The diagnostics screen (PRD §6.9): how much detail Spidola records, the versions a support thread
 * needs to name this build, and a viewer over the recent activity.
 *
 * Activity is shown **on screen** rather than shared to a file, keeping parity with tvOS, which has
 * no user-visible file system (PRD §7). Versions sit above activity because the log can run to
 * hundreds of lines, and the versions are the short answer to the question people are usually here
 * to answer.
 */
@Composable
fun DiagnosticsScreen(
    access: SettingsAccess,
    appVersion: String,
    onOpenLogLevel: () -> Unit,
    onGoBack: () -> Unit,
    modifier: Modifier = Modifier,
    viewModel: DiagnosticsViewModel = viewModel(factory = DiagnosticsViewModel.factory(access, appVersion)),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()

    // Re-read on entry, so returning from the log-level picker shows the level just chosen and the
    // lines recorded since.
    LaunchedEffect(Unit) { viewModel.load() }

    Box(modifier = modifier.fillMaxSize().background(SpidolaPalette.Studio)) {
        when (val current = state) {
            // A report always has versions and a level, so `Empty` cannot occur — it shares the
            // loading arm rather than inventing a state the screen can never be in.
            LoadState.Loading, LoadState.Empty -> Centered(stringResource(R.string.diagnostics_loading))
            is LoadState.Failed ->
                ActionableErrorContent(current.error, onRetry = viewModel::load, onGoBack = onGoBack)
            is LoadState.Ready -> DiagnosticsContent(report = current.value, onOpenLogLevel = onOpenLogLevel)
        }
    }
}

@Composable
private fun DiagnosticsContent(
    report: DiagnosticsReport,
    onOpenLogLevel: () -> Unit,
) {
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
                text = stringResource(R.string.diagnostics_title),
                style = MaterialTheme.typography.displayLarge,
                color = SpidolaPalette.BroadcastWhite,
                modifier = Modifier.semantics { heading() },
            )
        }
        item(key = "log-level") {
            LogLevelRow(
                level = report.logLevel,
                onClick = onOpenLogLevel,
                modifier = Modifier.focusRequester(firstRow).testTag("diagnostics-log-level"),
            )
        }

        section(R.string.diagnostics_versions)
        versions(report.versions)

        section(R.string.diagnostics_recent_activity)
        activity(report.activity)
    }
}

/**
 * The recorded-detail row. It reuses the picker vocabulary rather than a bespoke control, so the
 * level is chosen exactly the way every other closed-set setting is.
 */
@Composable
private fun LogLevelRow(
    level: LogLevel,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val value = SettingValue.Logging(level).label()
    SpidolaRow(
        title = stringResource(R.string.diagnostics_log_level),
        subtitle = stringResource(R.string.diagnostics_log_level_explainer),
        accessory = RowAccessory.Label(value),
        onClick = onClick,
        modifier = modifier.semantics { stateDescription = value },
    )
}

/**
 * The versions block. Numerals are tabular here because the whole type scale sets `tnum`
 * (PRD §8.3), so the labels and values line up column-wise without a font of their own.
 */
private fun LazyListScope.versions(versions: DiagnosticsVersions) {
    val rows =
        listOf(
            R.string.diagnostics_version_app to versions.app,
            R.string.diagnostics_version_core to versions.core,
            R.string.diagnostics_version_core_revision to versions.coreRevision,
            R.string.diagnostics_version_schema to versions.schema.toString(),
            R.string.diagnostics_version_boundary to versions.boundary.toString(),
        )
    items(rows.size, key = { index -> "version-${rows[index].first}" }) { index ->
        val (label, value) = rows[index]
        VersionRow(label = label, value = value)
    }
}

@Composable
private fun VersionRow(
    @StringRes label: Int,
    value: String,
) {
    Row(
        modifier = Modifier.fillMaxWidth().padding(vertical = SpidolaSpacing.xs),
        horizontalArrangement = Arrangement.spacedBy(SpidolaSpacing.m),
    ) {
        Text(
            text = stringResource(label),
            style = MaterialTheme.typography.labelMedium,
            color = SpidolaPalette.Static,
            modifier = Modifier.widthIn(min = 220.dp),
        )
        Text(
            text = value,
            style = MaterialTheme.typography.labelMedium,
            color = SpidolaPalette.BroadcastWhite,
        )
    }
}

/**
 * The recent-activity viewer. Each line is focusable — not because a line does anything, but because
 * on TV focus *is* the cursor: an unfocusable list cannot be reached or scrolled with a D-pad, and
 * a screen reader would never reach the lines either.
 */
private fun LazyListScope.activity(lines: ImmutableList<String>) {
    if (lines.isEmpty()) {
        item(key = "activity-empty") {
            Text(
                text = stringResource(R.string.diagnostics_recent_activity_empty),
                style = MaterialTheme.typography.bodyLarge,
                color = SpidolaPalette.Static,
                modifier = Modifier.padding(SpidolaSpacing.m),
            )
        }
        return
    }
    itemsIndexed(lines, key = { index, _ -> "activity-$index" }) { index, line ->
        ActivityLine(line = line, modifier = Modifier.testTag("activity-$index"))
    }
}

@Composable
private fun ActivityLine(
    line: String,
    modifier: Modifier = Modifier,
) {
    val interactionSource = remember { MutableInteractionSource() }
    val isFocused by interactionSource.collectIsFocusedAsState()
    Text(
        text = line,
        style = MaterialTheme.typography.labelMedium,
        color = if (isFocused) SpidolaPalette.BroadcastWhite else SpidolaPalette.Static,
        modifier =
            modifier
                .fillMaxWidth()
                .border(
                    width = SpidolaFocus.borderWidth,
                    color = if (isFocused) SpidolaPalette.TestCardAmber else Color.Transparent,
                    shape = SpidolaFocus.cardShape,
                ).padding(SpidolaSpacing.s)
                .focusable(interactionSource = interactionSource),
    )
}
