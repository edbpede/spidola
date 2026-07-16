// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.feature.browse

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.tv.material3.MaterialTheme
import androidx.tv.material3.Text
import dev.spidola.tv.core.corekit.CustomChannelInput
import dev.spidola.tv.core.corekit.CustomChannelsAccess
import dev.spidola.tv.core.corekit.CustomRequestHeader
import dev.spidola.tv.core.corekit.LoadState
import dev.spidola.tv.core.designsystem.RowAccessory
import dev.spidola.tv.core.designsystem.SpidolaPalette
import dev.spidola.tv.core.designsystem.SpidolaRow
import dev.spidola.tv.core.designsystem.SpidolaSpacing
import uniffi.core_api.CustomChannelSummary
import uniffi.core_api.CustomGroup
import uniffi.core_api.CustomImportMode

private sealed interface CustomEditor {
    data class Channel(
        val summary: CustomChannelSummary?,
    ) : CustomEditor

    data class Group(
        val group: CustomGroup?,
    ) : CustomEditor

    data object Share : CustomEditor
}

@Composable
fun CustomChannelsScreen(
    access: CustomChannelsAccess,
    onGoBack: () -> Unit,
    onShare: (String) -> Unit,
    onPlay: (CustomChannelSummary) -> Unit,
    modifier: Modifier = Modifier,
    viewModel: CustomChannelsViewModel = viewModel(factory = CustomChannelsViewModel.factory(access)),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    var editor by remember { mutableStateOf<CustomEditor?>(null) }
    Box(modifier.fillMaxSize().background(SpidolaPalette.Studio)) {
        val current = state
        when {
            editor is CustomEditor.Channel && current is LoadState.Ready ->
                ChannelEditor(
                    editor = editor as CustomEditor.Channel,
                    groups = current.value.groups,
                    onSave = { id, input ->
                        if (id == null) viewModel.createChannel(input) else viewModel.updateChannel(id, input)
                        editor = null
                    },
                    onCancel = { editor = null },
                )
            editor is CustomEditor.Group && current is LoadState.Ready ->
                GroupEditor(
                    editor = editor as CustomEditor.Group,
                    onSave = { id, name ->
                        if (id == null) viewModel.createGroup(name) else viewModel.renameGroup(id, name)
                        editor = null
                    },
                    onCancel = { editor = null },
                )
            editor == CustomEditor.Share && current is LoadState.Ready ->
                SharingEditor(
                    onExport = { viewModel.export(onShare) },
                    onImport = viewModel::import,
                    onCancel = { editor = null },
                )
            else ->
                Manager(
                    state = current,
                    onRetry = viewModel::load,
                    onGoBack = onGoBack,
                    onAddChannel = { editor = CustomEditor.Channel(null) },
                    onAddGroup = { editor = CustomEditor.Group(null) },
                    onShare = { editor = CustomEditor.Share },
                    onPlayChannel = onPlay,
                    onEditChannel = { editor = CustomEditor.Channel(it) },
                    onEditGroup = { editor = CustomEditor.Group(it) },
                    onDeleteChannel = viewModel::deleteChannel,
                    onDeleteGroup = viewModel::deleteGroup,
                    onMoveChannelUp = viewModel::moveChannelUp,
                    onMoveChannelDown = viewModel::moveChannelDown,
                    onMoveGroupUp = viewModel::moveGroupUp,
                    onMoveGroupDown = viewModel::moveGroupDown,
                )
        }
    }
}

@Suppress("LongParameterList")
@Composable
private fun Manager(
    state: LoadState<CustomCatalog>,
    onRetry: () -> Unit,
    onGoBack: () -> Unit,
    onAddChannel: () -> Unit,
    onAddGroup: () -> Unit,
    onShare: () -> Unit,
    onPlayChannel: (CustomChannelSummary) -> Unit,
    onEditChannel: (CustomChannelSummary) -> Unit,
    onEditGroup: (CustomGroup) -> Unit,
    onDeleteChannel: (Long) -> Unit,
    onDeleteGroup: (Long) -> Unit,
    onMoveChannelUp: (Int, Int) -> Unit,
    onMoveChannelDown: (Int, Int) -> Unit,
    onMoveGroupUp: (Int) -> Unit,
    onMoveGroupDown: (Int) -> Unit,
) {
    when (state) {
        LoadState.Loading -> CenteredMessage(stringResource(R.string.browse_custom_loading))
        LoadState.Empty -> Unit
        is LoadState.Failed -> ActionableErrorContent(state.error, onRetry, onGoBack)
        is LoadState.Ready ->
            Column(
                Modifier
                    .fillMaxSize()
                    .verticalScroll(rememberScrollState())
                    .padding(SpidolaSpacing.safeHorizontal, SpidolaSpacing.safeVertical),
                verticalArrangement = Arrangement.spacedBy(SpidolaSpacing.m),
            ) {
                Text(
                    stringResource(R.string.browse_custom_title),
                    style = MaterialTheme.typography.displayLarge,
                    color = SpidolaPalette.BroadcastWhite,
                )
                Row(horizontalArrangement = Arrangement.spacedBy(SpidolaSpacing.m)) {
                    SpidolaRow(stringResource(R.string.browse_custom_add), onAddChannel, Modifier.weight(1f))
                    SpidolaRow(stringResource(R.string.browse_custom_add_group), onAddGroup, Modifier.weight(1f))
                    SpidolaRow(stringResource(R.string.browse_custom_share), onShare, Modifier.weight(1f))
                }
                state.value.groups.forEachIndexed { index, group ->
                    Row(horizontalArrangement = Arrangement.spacedBy(SpidolaSpacing.m)) {
                        SpidolaRow(group.name, { onEditGroup(group) }, Modifier.weight(1f))
                        CompactAction(R.string.browse_favorites_move_up) { onMoveGroupUp(index) }
                        CompactAction(R.string.browse_favorites_move_down) { onMoveGroupDown(index) }
                        CompactAction(R.string.browse_custom_delete) { onDeleteGroup(group.id) }
                    }
                }
                state.value.sections.forEachIndexed { sectionIndex, section ->
                    Text(
                        section.group?.name ?: stringResource(R.string.browse_custom_ungrouped),
                        style = MaterialTheme.typography.titleLarge,
                        color = SpidolaPalette.Static,
                    )
                    section.channels.forEachIndexed { index, channel ->
                        Row(horizontalArrangement = Arrangement.spacedBy(SpidolaSpacing.m)) {
                            SpidolaRow(
                                title = channel.name,
                                subtitle =
                                    stringResource(
                                        R.string.browse_custom_request_count,
                                        channel.headerCount.toInt(),
                                    ),
                                accessory = RowAccessory.Label((index + 1).toString().padStart(2, '0')),
                                onClick = { onPlayChannel(channel) },
                                modifier = Modifier.weight(1f).testTag("custom-${channel.id}"),
                            )
                            CompactAction(R.string.browse_custom_edit) { onEditChannel(channel) }
                            CompactAction(R.string.browse_favorites_move_up) {
                                onMoveChannelUp(sectionIndex, index)
                            }
                            CompactAction(R.string.browse_favorites_move_down) {
                                onMoveChannelDown(sectionIndex, index)
                            }
                            CompactAction(R.string.browse_custom_delete) { onDeleteChannel(channel.id) }
                        }
                    }
                }
            }
    }
}

@Composable
private fun CompactAction(
    label: Int,
    onClick: () -> Unit,
) {
    SpidolaRow(stringResource(label), onClick, Modifier.width(170.dp))
}

@Composable
private fun GroupEditor(
    editor: CustomEditor.Group,
    onSave: (Long?, String) -> Unit,
    onCancel: () -> Unit,
) {
    var name by remember { mutableStateOf(editor.group?.name.orEmpty()) }
    EditorFrame(stringResource(R.string.browse_custom_group_editor)) {
        ControlRoomField(stringResource(R.string.browse_custom_name), name, { name = it }, "custom-group-name")
        SpidolaRow(stringResource(R.string.browse_custom_save_group), { onSave(editor.group?.id, name) })
        SpidolaRow(stringResource(R.string.browse_custom_cancel), onCancel)
    }
}

@Composable
private fun ChannelEditor(
    editor: CustomEditor.Channel,
    groups: List<CustomGroup>,
    onSave: (Long?, CustomChannelInput) -> Unit,
    onCancel: () -> Unit,
) {
    var name by remember { mutableStateOf(editor.summary?.name.orEmpty()) }
    var locator by remember { mutableStateOf("") }
    var logo by remember { mutableStateOf(editor.summary?.logo.orEmpty()) }
    var groupId by remember { mutableStateOf(editor.summary?.groupId) }
    var requestDetails by remember { mutableStateOf(false) }
    var userAgent by remember { mutableStateOf("") }
    var headers by remember { mutableStateOf("") }
    EditorFrame(stringResource(R.string.browse_custom_editor)) {
        ControlRoomField(stringResource(R.string.browse_custom_name), name, { name = it }, "custom-name")
        ControlRoomField(
            stringResource(R.string.browse_custom_address),
            locator,
            { locator = it },
            "custom-address",
        )
        ControlRoomField(stringResource(R.string.browse_custom_logo), logo, { logo = it }, "custom-logo")
        SpidolaRow(
            title = stringResource(R.string.browse_custom_group),
            accessory =
                RowAccessory.Label(
                    groups.firstOrNull { it.id == groupId }?.name
                        ?: stringResource(R.string.browse_custom_ungrouped),
                ),
            onClick = {
                val ids = listOf<Long?>(null) + groups.map { it.id }
                val index = ids.indexOf(groupId).coerceAtLeast(0)
                groupId = ids[(index + 1) % ids.size]
            },
        )
        SpidolaRow(
            title = stringResource(R.string.browse_custom_request_details),
            accessory = RowAccessory.Label(if (requestDetails) "−" else "+"),
            onClick = { requestDetails = !requestDetails },
        )
        if (requestDetails) {
            ControlRoomField(
                stringResource(R.string.browse_custom_browser_identity),
                userAgent,
                { userAgent = it },
                "custom-user-agent",
            )
            ControlRoomField(
                stringResource(R.string.browse_custom_request_lines),
                headers,
                { headers = it },
                "custom-headers",
            )
        }
        if (editor.summary != null) {
            Text(
                stringResource(R.string.browse_custom_reenter_address),
                style = MaterialTheme.typography.labelMedium,
                color = SpidolaPalette.Static,
            )
        }
        SpidolaRow(
            stringResource(R.string.browse_custom_save),
            onClick = {
                onSave(
                    editor.summary?.id,
                    CustomChannelInput(
                        groupId = groupId,
                        name = name.trim(),
                        logo = logo.trim().ifBlank { null },
                        locator = locator.trim(),
                        userAgent = userAgent.trim().ifBlank { null },
                        headers = parseHeaders(headers),
                    ),
                )
            },
            modifier = Modifier.testTag("custom-save"),
        )
        SpidolaRow(stringResource(R.string.browse_custom_cancel), onCancel)
    }
}

@Composable
private fun SharingEditor(
    onExport: () -> Unit,
    onImport: (String, CustomImportMode) -> Unit,
    onCancel: () -> Unit,
) {
    var contents by remember { mutableStateOf("") }
    EditorFrame(stringResource(R.string.browse_custom_share)) {
        Text(
            stringResource(R.string.browse_custom_share_explainer),
            style = MaterialTheme.typography.bodyLarge,
            color = SpidolaPalette.Static,
        )
        SpidolaRow(stringResource(R.string.browse_custom_export), onExport)
        ControlRoomField(
            stringResource(R.string.browse_custom_import_text),
            contents,
            { contents = it },
            "custom-import",
        )
        SpidolaRow(stringResource(R.string.browse_custom_import_merge), { onImport(contents, CustomImportMode.MERGE) })
        SpidolaRow(
            stringResource(R.string.browse_custom_import_replace),
            { onImport(contents, CustomImportMode.REPLACE) },
        )
        SpidolaRow(stringResource(R.string.browse_custom_cancel), onCancel)
    }
}

@Composable
private fun EditorFrame(
    title: String,
    content: @Composable () -> Unit,
) {
    Column(
        Modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(SpidolaSpacing.safeHorizontal, SpidolaSpacing.safeVertical),
        verticalArrangement = Arrangement.spacedBy(SpidolaSpacing.m),
    ) {
        Text(title, style = MaterialTheme.typography.displayLarge, color = SpidolaPalette.BroadcastWhite)
        content()
    }
}

@Composable
private fun ControlRoomField(
    label: String,
    value: String,
    onValueChange: (String) -> Unit,
    tag: String,
) {
    Column(verticalArrangement = Arrangement.spacedBy(SpidolaSpacing.s)) {
        Text(label, style = MaterialTheme.typography.labelMedium, color = SpidolaPalette.Static)
        BasicTextField(
            value = value,
            onValueChange = onValueChange,
            textStyle = MaterialTheme.typography.bodyLarge.merge(TextStyle(color = SpidolaPalette.BroadcastWhite)),
            cursorBrush = SolidColor(SpidolaPalette.TestCardAmber),
            modifier =
                Modifier
                    .fillMaxWidth()
                    .background(SpidolaPalette.Set)
                    .padding(SpidolaSpacing.m)
                    .testTag(tag),
        )
    }
}

private fun parseHeaders(text: String): List<CustomRequestHeader> =
    text.lineSequence().mapNotNull { line ->
        val split = line.indexOf(':')
        if (split <= 0) null else CustomRequestHeader(line.take(split).trim(), line.drop(split + 1).trim())
    }.toList()
