// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.feature.sources

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.TextStyle
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.tv.material3.MaterialTheme
import androidx.tv.material3.Text
import dev.spidola.tv.core.corekit.LoadState
import dev.spidola.tv.core.corekit.SourcesAccess
import dev.spidola.tv.core.corekit.common
import dev.spidola.tv.core.corekit.id
import dev.spidola.tv.core.corekit.isRefreshable
import dev.spidola.tv.core.corekit.name
import dev.spidola.tv.core.designsystem.RowAccessory
import dev.spidola.tv.core.designsystem.SpidolaPalette
import dev.spidola.tv.core.designsystem.SpidolaRow
import dev.spidola.tv.core.designsystem.SpidolaSpacing
import kotlinx.collections.immutable.ImmutableList
import uniffi.core_api.Source

/**
 * The manage-sources screen: add a source, then rename / enable-disable / refresh / delete / set
 * auto-refresh on each (PRD §6.1). Selecting a source expands its actions inline (a D-pad-friendly
 * alternative to a context menu); refresh streams through the core and preserves favorites and
 * hidden flags (§4.4).
 */
@Suppress("ViewModelForwarding") // The screen owns the VM and hands it to its list children.
@Composable
fun SourcesScreen(
    access: SourcesAccess,
    onAddSource: () -> Unit,
    onPairPhone: () -> Unit,
    modifier: Modifier = Modifier,
    isActive: Boolean = true,
    viewModel: SourcesViewModel = viewModel(factory = SourcesViewModel.factory(access)),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val refreshing by viewModel.refreshing.collectAsStateWithLifecycle()
    val status by viewModel.status.collectAsStateWithLifecycle()

    LaunchedEffect(isActive) {
        if (isActive) viewModel.load()
    }

    var expandedId by remember { mutableStateOf<Long?>(null) }
    var renamingId by remember { mutableStateOf<Long?>(null) }
    var confirmDeleteId by remember { mutableStateOf<Long?>(null) }
    var renameText by remember { mutableStateOf("") }

    Box(modifier = modifier.fillMaxSize().background(SpidolaPalette.Studio)) {
        when (val current = state) {
            LoadState.Loading -> Centered(stringResource(R.string.sources_loading))
            is LoadState.Failed ->
                ActionableErrorContent(current.error, onRetry = viewModel::load, onGoBack = onAddSource)
            LoadState.Empty -> EmptySources(status, onAddSource, onPairPhone)
            is LoadState.Ready -> {
                SourceList(
                    sources = current.value,
                    refreshing = refreshing,
                    status = status,
                    onAddSource = onAddSource,
                    onPairPhone = onPairPhone,
                    expandedId = expandedId,
                    onToggleExpanded = { id -> expandedId = if (expandedId == id) null else id },
                    renamingId = renamingId,
                    renameText = renameText,
                    onRenameTextChange = { renameText = it },
                    onStartRename = { source ->
                        renamingId = source.id
                        renameText = source.name
                    },
                    onCommitRename = { id ->
                        viewModel.rename(id, renameText)
                        renamingId = null
                    },
                    confirmDeleteId = confirmDeleteId,
                    onDeleteClick = { id ->
                        if (confirmDeleteId == id) {
                            viewModel.delete(id)
                            confirmDeleteId = null
                            expandedId = null
                        } else {
                            confirmDeleteId = id
                        }
                    },
                    viewModel = viewModel,
                )
            }
        }
    }
}

@Suppress("LongParameterList", "ViewModelForwarding", "ParameterNaming")
@Composable
private fun SourceList(
    sources: ImmutableList<Source>,
    refreshing: Set<Long>,
    status: String?,
    onAddSource: () -> Unit,
    onPairPhone: () -> Unit,
    expandedId: Long?,
    onToggleExpanded: (Long) -> Unit,
    renamingId: Long?,
    renameText: String,
    onRenameTextChange: (String) -> Unit,
    onStartRename: (Source) -> Unit,
    onCommitRename: (Long) -> Unit,
    confirmDeleteId: Long?,
    onDeleteClick: (Long) -> Unit,
    viewModel: SourcesViewModel,
) {
    LazyColumn(
        modifier = Modifier.fillMaxSize(),
        contentPadding =
            PaddingValues(
                horizontal = SpidolaSpacing.safeHorizontal,
                vertical = SpidolaSpacing.safeVertical,
            ),
        verticalArrangement = Arrangement.spacedBy(SpidolaSpacing.s),
    ) {
        status?.let {
            item {
                Text(text = it, style = MaterialTheme.typography.labelMedium, color = SpidolaPalette.TestCardAmber)
            }
        }
        item {
            SpidolaRow(
                title = stringResource(R.string.sources_add),
                onClick = onAddSource,
                modifier = Modifier.testTag("sources-add"),
            )
        }
        item {
            SpidolaRow(
                title = stringResource(R.string.pairing_title),
                onClick = onPairPhone,
                modifier = Modifier.testTag("sources-pair"),
            )
        }
        items(sources, key = { it.id }) { source ->
            SourceItem(
                source = source,
                isRefreshing = source.id in refreshing,
                expanded = expandedId == source.id,
                onToggleExpanded = { onToggleExpanded(source.id) },
                renaming = renamingId == source.id,
                renameText = renameText,
                onRenameTextChange = onRenameTextChange,
                onStartRename = { onStartRename(source) },
                onCommitRename = { onCommitRename(source.id) },
                confirmDelete = confirmDeleteId == source.id,
                onDeleteClick = { onDeleteClick(source.id) },
                viewModel = viewModel,
            )
        }
    }
}

@Suppress("LongParameterList", "ViewModelForwarding", "ParameterNaming", "MultipleEmitters")
@Composable
private fun SourceItem(
    source: Source,
    isRefreshing: Boolean,
    expanded: Boolean,
    onToggleExpanded: () -> Unit,
    renaming: Boolean,
    renameText: String,
    onRenameTextChange: (String) -> Unit,
    onStartRename: () -> Unit,
    onCommitRename: () -> Unit,
    confirmDelete: Boolean,
    onDeleteClick: () -> Unit,
    viewModel: SourcesViewModel,
) {
    val autoRefresh = AutoRefreshOption.from(source.common.autoRefreshSecs)
    val condition =
        when {
            isRefreshing -> stringResource(R.string.sources_refreshing)
            !source.common.enabled -> stringResource(R.string.sources_disabled)
            else -> null
        }
    SpidolaRow(
        title = source.name,
        subtitle = stringResource(R.string.sources_subtitle, source.kindLabel(), stringResource(autoRefresh.label)),
        accessory = if (condition != null) RowAccessory.Label(condition) else RowAccessory.None,
        onClick = onToggleExpanded,
        // Refreshing and disabled are what this source *is* at the moment, so they announce as the
        // row's state. Left in the accessory alone they would land inside the name — and "Disabled"
        // read as part of a name collides with the word TalkBack already uses for a control that
        // cannot be pressed, which this row very much can (PRD §6.10).
        modifier =
            Modifier
                .semantics { condition?.let { stateDescription = it } }
                .testTag("manage-source-${source.name}"),
    )
    if (!expanded) return

    val actionModifier = Modifier.padding(start = SpidolaSpacing.l)
    if (renaming) {
        RenameField(renameText, onRenameTextChange)
        SpidolaRow(
            title = stringResource(R.string.sources_save_name),
            onClick = onCommitRename,
            modifier = actionModifier,
        )
    } else {
        SpidolaRow(
            title = stringResource(R.string.sources_rename),
            onClick = onStartRename,
            modifier = actionModifier,
        )
    }
    SpidolaRow(
        title = stringResource(if (source.common.enabled) R.string.sources_disable else R.string.sources_enable),
        onClick = { viewModel.setEnabled(source.id, !source.common.enabled) },
        modifier = actionModifier,
    )
    if (source.isRefreshable) {
        SpidolaRow(
            title = stringResource(R.string.sources_refresh_now),
            onClick = { viewModel.refresh(source) },
            modifier = actionModifier,
        )
        SpidolaRow(
            title = stringResource(R.string.sources_auto_refresh, stringResource(autoRefresh.label)),
            onClick = { viewModel.setAutoRefresh(source.id, autoRefresh.next()) },
            modifier = actionModifier,
        )
    }
    val deleteWarning = stringResource(R.string.sources_delete_warning)
    SpidolaRow(
        title = stringResource(if (confirmDelete) R.string.sources_delete_confirm else R.string.sources_delete),
        accessory = if (confirmDelete) RowAccessory.Label(deleteWarning) else RowAccessory.None,
        onClick = onDeleteClick,
        // The warning is the armed state, not decoration: this row's second press is the one that
        // cannot be taken back, and a listener has to hear that before pressing rather than after.
        modifier = actionModifier.semantics { if (confirmDelete) stateDescription = deleteWarning },
    )
}

@Composable
private fun RenameField(
    value: String,
    onValueChange: (String) -> Unit,
) {
    Box(
        modifier =
            Modifier
                .fillMaxWidth()
                .padding(start = SpidolaSpacing.l)
                .background(SpidolaPalette.Set)
                .padding(SpidolaSpacing.m),
    ) {
        BasicTextField(
            value = value,
            onValueChange = onValueChange,
            singleLine = true,
            textStyle = MaterialTheme.typography.bodyLarge.merge(TextStyle(color = SpidolaPalette.BroadcastWhite)),
            cursorBrush = SolidColor(SpidolaPalette.TestCardAmber),
            modifier = Modifier.fillMaxWidth().testTag("rename-field"),
        )
    }
}

@Composable
private fun EmptySources(
    status: String?,
    onAddSource: () -> Unit,
    onPairPhone: () -> Unit,
) {
    LazyColumn(
        modifier = Modifier.fillMaxSize(),
        contentPadding =
            PaddingValues(
                horizontal = SpidolaSpacing.safeHorizontal,
                vertical = SpidolaSpacing.safeVertical,
            ),
        verticalArrangement = Arrangement.spacedBy(SpidolaSpacing.s),
    ) {
        status?.let {
            item {
                Text(text = it, style = MaterialTheme.typography.labelMedium, color = SpidolaPalette.TestCardAmber)
            }
        }
        item {
            SpidolaRow(
                title = stringResource(R.string.sources_add),
                onClick = onAddSource,
                modifier = Modifier.testTag("sources-add"),
            )
        }
        item {
            SpidolaRow(
                title = stringResource(R.string.pairing_title),
                onClick = onPairPhone,
                modifier = Modifier.testTag("sources-pair"),
            )
        }
        item {
            Text(
                text = stringResource(R.string.sources_empty),
                style = MaterialTheme.typography.bodyLarge,
                color = SpidolaPalette.Static,
                modifier = Modifier.padding(SpidolaSpacing.m),
            )
        }
    }
}

@Composable
private fun Centered(message: String) {
    Box(modifier = Modifier.fillMaxSize().padding(SpidolaSpacing.xl)) {
        Text(text = message, style = MaterialTheme.typography.titleLarge, color = SpidolaPalette.BroadcastWhite)
    }
}
