// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.feature.sources

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.tv.material3.MaterialTheme
import androidx.tv.material3.Text
import dev.spidola.tv.core.corekit.SourcesAccess
import dev.spidola.tv.core.designsystem.RowAccessory
import dev.spidola.tv.core.designsystem.SpidolaPalette
import dev.spidola.tv.core.designsystem.SpidolaRow
import dev.spidola.tv.core.designsystem.SpidolaSpacing
import uniffi.core_api.ImportOutcome
import uniffi.core_api.ImportStage

/**
 * The add-source screen: choose URL or paste, enter the details, and watch a live import with a
 * cancel button and a diagnostics summary (PRD §6.1). Xtream accounts and LAN pairing land in
 * Phase 6. The screen owns the form fields; the view model owns the import phase.
 */
@Suppress("ParameterNaming") // onFinished reads naturally as the completion callback here.
@Composable
fun AddSourceScreen(
    access: SourcesAccess,
    onFinished: () -> Unit,
    modifier: Modifier = Modifier,
    viewModel: AddSourceViewModel = viewModel(factory = AddSourceViewModel.factory(access)),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val validation by viewModel.validation.collectAsStateWithLifecycle()

    var mode by rememberSaveable { mutableStateOf(AddSourceMode.URL) }
    var name by rememberSaveable { mutableStateOf("") }
    var url by rememberSaveable { mutableStateOf("") }
    var content by rememberSaveable { mutableStateOf("") }
    var userAgent by rememberSaveable { mutableStateOf("") }
    var acceptInvalidTls by rememberSaveable { mutableStateOf(false) }

    fun form() = AddSourceForm(mode, name, url, content, userAgent, acceptInvalidTls)

    Box(modifier = modifier.fillMaxSize().background(SpidolaPalette.Studio)) {
        when (val current = state) {
            AddSourceState.Editing ->
                Form(
                    mode = mode,
                    onModeChange = { mode = it },
                    name = name,
                    onNameChange = { name = it },
                    url = url,
                    onUrlChange = { url = it },
                    content = content,
                    onContentChange = { content = it },
                    userAgent = userAgent,
                    onUserAgentChange = { userAgent = it },
                    acceptInvalidTls = acceptInvalidTls,
                    onToggleTls = { acceptInvalidTls = !acceptInvalidTls },
                    validation = validation,
                    onSubmit = { viewModel.submit(form()) },
                )
            is AddSourceState.Importing -> Importing(current.stage, current.channels, viewModel::cancel)
            is AddSourceState.Done -> Done(current.outcome, onFinished)
            is AddSourceState.Failed ->
                ActionableErrorContent(
                    error = current.error,
                    onRetry = { viewModel.submit(form()) },
                    onGoBack = onFinished,
                )
        }
    }
}

@Suppress("LongParameterList")
@Composable
private fun Form(
    mode: AddSourceMode,
    onModeChange: (AddSourceMode) -> Unit,
    name: String,
    onNameChange: (String) -> Unit,
    url: String,
    onUrlChange: (String) -> Unit,
    content: String,
    onContentChange: (String) -> Unit,
    userAgent: String,
    onUserAgentChange: (String) -> Unit,
    acceptInvalidTls: Boolean,
    onToggleTls: () -> Unit,
    validation: String?,
    onSubmit: () -> Unit,
) {
    Column(
        modifier =
            Modifier
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .padding(horizontal = SpidolaSpacing.safeHorizontal, vertical = SpidolaSpacing.safeVertical)
                .widthIn(max = 1100.dp),
        verticalArrangement = Arrangement.spacedBy(SpidolaSpacing.m),
    ) {
        Row(horizontalArrangement = Arrangement.spacedBy(SpidolaSpacing.m)) {
            AddSourceMode.entries.forEach { candidate ->
                SpidolaRow(
                    title = candidate.title,
                    accessory =
                        if (candidate == mode) {
                            RowAccessory.Star
                        } else {
                            RowAccessory.None
                        },
                    onClick = { onModeChange(candidate) },
                    modifier = Modifier.weight(1f),
                )
            }
        }
        LabeledField("Name", name, onNameChange, "add-source-name")
        when (mode) {
            AddSourceMode.URL -> {
                LabeledField("Playlist URL", url, onUrlChange, "add-source-url")
                LabeledField("User agent (optional)", userAgent, onUserAgentChange, "add-source-userAgent")
                SpidolaRow(
                    title = "Allow self-signed certificates",
                    accessory = RowAccessory.Label(if (acceptInvalidTls) "On" else "Off"),
                    onClick = onToggleTls,
                )
            }
            AddSourceMode.FILE -> LabeledField("Paste playlist text", content, onContentChange, "add-source-content")
        }
        if (validation != null) {
            Text(text = validation, style = MaterialTheme.typography.labelMedium, color = SpidolaPalette.StreamRed)
        }
        SpidolaRow(title = "Add source", onClick = onSubmit, modifier = Modifier.testTag("add-source-submit"))
    }
}

@Composable
private fun LabeledField(
    label: String,
    value: String,
    onValueChange: (String) -> Unit,
    tag: String,
) {
    Box(
        modifier =
            Modifier
                .fillMaxWidth()
                .background(SpidolaPalette.Set)
                .padding(SpidolaSpacing.m),
    ) {
        if (value.isEmpty()) {
            Text(text = label, style = MaterialTheme.typography.bodyLarge, color = SpidolaPalette.Static)
        }
        BasicTextField(
            value = value,
            onValueChange = onValueChange,
            singleLine = false,
            textStyle = MaterialTheme.typography.bodyLarge.merge(TextStyle(color = SpidolaPalette.BroadcastWhite)),
            cursorBrush = SolidColor(SpidolaPalette.TestCardAmber),
            modifier = Modifier.fillMaxWidth().testTag(tag),
        )
    }
}

@Composable
private fun Importing(
    stage: ImportStage,
    channels: ULong,
    onCancel: () -> Unit,
) {
    Column(
        modifier = Modifier.fillMaxSize().padding(SpidolaSpacing.xl),
        verticalArrangement = Arrangement.spacedBy(SpidolaSpacing.l, Alignment.CenterVertically),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text(
            text = stageLabel(stage, channels),
            style = MaterialTheme.typography.titleLarge,
            color = SpidolaPalette.BroadcastWhite,
        )
        SpidolaRow(title = "Cancel", onClick = onCancel, modifier = Modifier.testTag("add-source-cancel"))
    }
}

@Suppress("ParameterNaming") // onFinished reads naturally as the completion callback here.
@Composable
private fun Done(
    outcome: ImportOutcome,
    onFinished: () -> Unit,
) {
    val skipped = outcome.skipped + outcome.invalid
    Column(
        modifier = Modifier.fillMaxSize().padding(SpidolaSpacing.xl),
        verticalArrangement = Arrangement.spacedBy(SpidolaSpacing.l, Alignment.CenterVertically),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text(
            text = "Added ${outcome.inserted} channels",
            style = MaterialTheme.typography.titleLarge,
            color = SpidolaPalette.BroadcastWhite,
        )
        if (skipped > 0uL) {
            Text(
                text = "$skipped entries were skipped as unreadable.",
                style = MaterialTheme.typography.labelMedium,
                color = SpidolaPalette.Static,
            )
        }
        SpidolaRow(title = "Done", onClick = onFinished, modifier = Modifier.testTag("add-source-done"))
    }
}

private fun stageLabel(
    stage: ImportStage,
    channels: ULong,
): String =
    when (stage) {
        ImportStage.CONNECTING -> "Connecting…"
        ImportStage.DOWNLOADING -> "Importing… $channels channels"
        ImportStage.FINALIZING -> "Finishing up…"
    }
