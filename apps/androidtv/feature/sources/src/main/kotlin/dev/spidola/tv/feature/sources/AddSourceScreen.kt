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
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
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
 * The add-source screen: choose a playlist URL, pasted text, or an Xtream account, enter the
 * details, and watch a live import with a cancel button and a diagnostics summary (PRD §6.1). The
 * screen owns the form fields; the view model owns the import phase.
 *
 * [prefill] carries what a phone sent over LAN pairing. It fills the form and nothing more —
 * someone at the TV still presses Add, because a device on the network does not get to add a source
 * on its own.
 */
@Suppress("ParameterNaming") // onFinished reads naturally as the completion callback here.
@Composable
fun AddSourceScreen(
    access: SourcesAccess,
    onFinished: () -> Unit,
    modifier: Modifier = Modifier,
    prefill: AddSourcePrefill? = null,
    viewModel: AddSourceViewModel = viewModel(factory = AddSourceViewModel.factory(access)),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val validation by viewModel.validation.collectAsStateWithLifecycle()

    var mode by rememberSaveable { mutableStateOf(prefill?.mode ?: AddSourceMode.URL) }
    var name by rememberSaveable { mutableStateOf("") }
    var url by rememberSaveable { mutableStateOf(prefill?.url.orEmpty()) }
    var content by rememberSaveable { mutableStateOf("") }
    var userAgent by rememberSaveable { mutableStateOf("") }
    var acceptInvalidTls by rememberSaveable { mutableStateOf(false) }
    var server by rememberSaveable { mutableStateOf(prefill?.server.orEmpty()) }
    var username by rememberSaveable { mutableStateOf(prefill?.username.orEmpty()) }
    // `remember`, deliberately not `rememberSaveable`: saveable state is serialized to disk by the
    // framework, and a password belongs in memory only, in flight to `addXtream` (TECH_SPEC §12).
    // The cost is that a process death empties this one field, which is the right trade.
    var password by remember { mutableStateOf(prefill?.password.orEmpty()) }

    fun form() = AddSourceForm(mode, name, url, content, userAgent, acceptInvalidTls, server, username, password)

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
                    server = server,
                    onServerChange = { server = it },
                    username = username,
                    onUsernameChange = { username = it },
                    password = password,
                    onPasswordChange = { password = it },
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
    server: String,
    onServerChange: (String) -> Unit,
    username: String,
    onUsernameChange: (String) -> Unit,
    password: String,
    onPasswordChange: (String) -> Unit,
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
                    title = stringResource(candidate.title),
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
        LabeledField(stringResource(R.string.add_source_name), name, onNameChange, "add-source-name")
        when (mode) {
            AddSourceMode.URL -> {
                LabeledField(stringResource(R.string.add_source_url_field), url, onUrlChange, "add-source-url")
                LabeledField(
                    stringResource(R.string.add_source_user_agent),
                    userAgent,
                    onUserAgentChange,
                    "add-source-userAgent",
                )
                SpidolaRow(
                    title = stringResource(R.string.add_source_allow_self_signed),
                    accessory =
                        RowAccessory.Label(
                            stringResource(if (acceptInvalidTls) R.string.add_source_on else R.string.add_source_off),
                        ),
                    onClick = onToggleTls,
                )
            }
            AddSourceMode.FILE ->
                LabeledField(
                    stringResource(R.string.add_source_paste),
                    content,
                    onContentChange,
                    "add-source-content",
                )
            AddSourceMode.XTREAM -> {
                LabeledField(stringResource(R.string.add_source_server), server, onServerChange, "add-source-server")
                LabeledField(
                    stringResource(R.string.add_source_username),
                    username,
                    onUsernameChange,
                    "add-source-username",
                )
                LabeledField(
                    label = stringResource(R.string.add_source_password),
                    value = password,
                    onValueChange = onPasswordChange,
                    tag = "add-source-password",
                    masked = true,
                )
            }
        }
        if (validation != null) {
            Text(text = validation, style = MaterialTheme.typography.labelMedium, color = SpidolaPalette.StreamRed)
        }
        SpidolaRow(
            title = stringResource(R.string.add_source_submit),
            onClick = onSubmit,
            modifier = Modifier.testTag("add-source-submit"),
        )
    }
}

/** A labelled text field. [masked] renders a password: dots on screen, and single-line because a
 * password has no lines. */
@Composable
private fun LabeledField(
    label: String,
    value: String,
    onValueChange: (String) -> Unit,
    tag: String,
    masked: Boolean = false,
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
            singleLine = masked,
            visualTransformation = if (masked) PasswordVisualTransformation() else VisualTransformation.None,
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
        SpidolaRow(
            title = stringResource(R.string.add_source_cancel),
            onClick = onCancel,
            modifier = Modifier.testTag("add-source-cancel"),
        )
    }
}

@Suppress("ParameterNaming") // onFinished reads naturally as the completion callback here.
@Composable
private fun Done(
    outcome: ImportOutcome,
    onFinished: () -> Unit,
) {
    val inserted = outcome.inserted.toCount()
    val skipped = (outcome.skipped + outcome.invalid).toCount()
    Column(
        modifier = Modifier.fillMaxSize().padding(SpidolaSpacing.xl),
        verticalArrangement = Arrangement.spacedBy(SpidolaSpacing.l, Alignment.CenterVertically),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text(
            text = pluralStringResource(R.plurals.add_source_added, inserted, inserted),
            style = MaterialTheme.typography.titleLarge,
            color = SpidolaPalette.BroadcastWhite,
        )
        if (skipped > 0) {
            Text(
                text = pluralStringResource(R.plurals.add_source_skipped, skipped, skipped),
                style = MaterialTheme.typography.labelMedium,
                color = SpidolaPalette.Static,
            )
        }
        SpidolaRow(
            title = stringResource(R.string.add_source_done),
            onClick = onFinished,
            modifier = Modifier.testTag("add-source-done"),
        )
    }
}

@Composable
private fun stageLabel(
    stage: ImportStage,
    channels: ULong,
): String =
    when (stage) {
        ImportStage.CONNECTING -> stringResource(R.string.add_source_stage_connecting)
        ImportStage.DOWNLOADING ->
            channels.toCount().let { seen -> pluralStringResource(R.plurals.add_source_stage_importing, seen, seen) }
        ImportStage.FINALIZING -> stringResource(R.string.add_source_stage_finalizing)
    }

/**
 * A core count as a plural quantity. The core counts channels in a `ULong` and Android's plural
 * rules take an `Int`; the catalogs this app is built for run to tens of thousands (PRD §9), so the
 * clamp is unreachable — it is here so that if one ever did overflow, the sentence reads "many"
 * rather than a negative number.
 */
private fun ULong.toCount(): Int = coerceAtMost(Int.MAX_VALUE.toULong()).toInt()
