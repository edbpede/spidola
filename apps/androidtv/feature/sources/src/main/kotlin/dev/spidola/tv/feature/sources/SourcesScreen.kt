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
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.testTag
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
import dev.spidola.tv.core.corekit.kindLabel
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
    modifier: Modifier = Modifier,
    viewModel: SourcesViewModel = viewModel(factory = SourcesViewModel.factory(access)),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val refreshing by viewModel.refreshing.collectAsStateWithLifecycle()
    val status by viewModel.status.collectAsStateWithLifecycle()

    var expandedId by remember { mutableStateOf<Long?>(null) }
    var renamingId by remember { mutableStateOf<Long?>(null) }
    var confirmDeleteId by remember { mutableStateOf<Long?>(null) }
    var renameText by remember { mutableStateOf("") }

    Box(modifier = modifier.fillMaxSize().background(SpidolaPalette.Studio)) {
        when (val current = state) {
            LoadState.Loading -> Centered("Loading sources…")
            is LoadState.Failed ->
                ActionableErrorContent(current.error, onRetry = viewModel::load, onGoBack = onAddSource)
            LoadState.Empty -> EmptySources(status, onAddSource)
            is LoadState.Ready -> {
                SourceList(
                    sources = current.value,
                    refreshing = refreshing,
                    status = status,
                    onAddSource = onAddSource,
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
            SpidolaRow(title = "Add a source", onClick = onAddSource, modifier = Modifier.testTag("sources-add"))
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
    SpidolaRow(
        title = source.name,
        subtitle = "${source.kindLabel} · ${autoRefresh.label}",
        accessory =
            when {
                isRefreshing -> RowAccessory.Label("Refreshing…")
                !source.common.enabled -> RowAccessory.Label("Disabled")
                else -> RowAccessory.None
            },
        onClick = onToggleExpanded,
        modifier = Modifier.testTag("manage-source-${source.name}"),
    )
    if (!expanded) return

    val actionModifier = Modifier.padding(start = SpidolaSpacing.l)
    if (renaming) {
        RenameField(renameText, onRenameTextChange)
        SpidolaRow(title = "Save name", onClick = onCommitRename, modifier = actionModifier)
    } else {
        SpidolaRow(title = "Rename", onClick = onStartRename, modifier = actionModifier)
    }
    SpidolaRow(
        title = if (source.common.enabled) "Disable" else "Enable",
        onClick = { viewModel.setEnabled(source.id, !source.common.enabled) },
        modifier = actionModifier,
    )
    if (source.isRefreshable) {
        SpidolaRow(title = "Refresh now", onClick = { viewModel.refresh(source) }, modifier = actionModifier)
        SpidolaRow(
            title = "Auto-refresh: ${autoRefresh.label}",
            onClick = { viewModel.setAutoRefresh(source.id, autoRefresh.next()) },
            modifier = actionModifier,
        )
    }
    SpidolaRow(
        title = if (confirmDelete) "Confirm delete" else "Delete",
        accessory = if (confirmDelete) RowAccessory.Label("Can't be undone") else RowAccessory.None,
        onClick = onDeleteClick,
        modifier = actionModifier,
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
            SpidolaRow(title = "Add a source", onClick = onAddSource, modifier = Modifier.testTag("sources-add"))
        }
        item {
            Text(
                text = "No sources yet.",
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
