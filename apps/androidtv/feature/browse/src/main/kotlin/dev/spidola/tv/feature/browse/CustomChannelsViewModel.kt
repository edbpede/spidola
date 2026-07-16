// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

package dev.spidola.tv.feature.browse

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import dev.spidola.tv.core.corekit.ActionableError
import dev.spidola.tv.core.corekit.CustomChannelInput
import dev.spidola.tv.core.corekit.CustomChannelsAccess
import dev.spidola.tv.core.corekit.LoadState
import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.toImmutableList
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import uniffi.core_api.ApiException
import uniffi.core_api.CustomChannelSummary
import uniffi.core_api.CustomGroup
import uniffi.core_api.CustomImportMode

data class CustomChannelSection(
    val group: CustomGroup?,
    val channels: ImmutableList<CustomChannelSummary>,
)

data class CustomCatalog(
    val groups: ImmutableList<CustomGroup>,
    val sections: ImmutableList<CustomChannelSection>,
)

class CustomChannelsViewModel(
    private val access: CustomChannelsAccess,
) : ViewModel() {
    private val _state = MutableStateFlow<LoadState<CustomCatalog>>(LoadState.Loading)
    val state: StateFlow<LoadState<CustomCatalog>> = _state.asStateFlow()

    init {
        load()
    }

    fun load() {
        viewModelScope.launch {
            _state.value = LoadState.Loading
            _state.value =
                try {
                    val groups = access.customGroups()
                    val sections =
                        buildList {
                            add(CustomChannelSection(null, access.customChannels(null).toImmutableList()))
                            groups.forEach { group ->
                                add(CustomChannelSection(group, access.customChannels(group.id).toImmutableList()))
                            }
                        }
                    LoadState.Ready(CustomCatalog(groups.toImmutableList(), sections.toImmutableList()))
                } catch (e: CancellationException) {
                    throw e
                } catch (e: ApiException) {
                    LoadState.Failed(ActionableError.from(e))
                }
        }
    }

    fun createGroup(name: String) = mutate { access.createCustomGroup(name.trim()) }

    fun renameGroup(
        id: Long,
        name: String,
    ) = mutate { access.renameCustomGroup(id, name.trim()) }

    fun deleteGroup(id: Long) = mutate { access.deleteCustomGroup(id) }

    fun moveGroupUp(index: Int) {
        val groups = catalog()?.groups ?: return
        if (index > 0) mutate { access.moveCustomGroupBefore(groups[index].id, groups[index - 1].id) }
    }

    fun moveGroupDown(index: Int) {
        val groups = catalog()?.groups ?: return
        if (index < groups.lastIndex) mutate { access.moveCustomGroupAfter(groups[index].id, groups[index + 1].id) }
    }

    fun createChannel(input: CustomChannelInput) = mutate { access.createCustomChannel(input) }

    fun updateChannel(
        id: Long,
        input: CustomChannelInput,
    ) = mutate { access.updateCustomChannel(id, input) }

    fun deleteChannel(id: Long) = mutate { access.deleteCustomChannel(id) }

    fun moveChannelUp(
        sectionIndex: Int,
        channelIndex: Int,
    ) {
        val channels = catalog()?.sections?.getOrNull(sectionIndex)?.channels ?: return
        if (channelIndex > 0) {
            mutate { access.moveCustomChannelBefore(channels[channelIndex].id, channels[channelIndex - 1].id) }
        }
    }

    fun moveChannelDown(
        sectionIndex: Int,
        channelIndex: Int,
    ) {
        val channels = catalog()?.sections?.getOrNull(sectionIndex)?.channels ?: return
        if (channelIndex < channels.lastIndex) {
            mutate { access.moveCustomChannelAfter(channels[channelIndex].id, channels[channelIndex + 1].id) }
        }
    }

    fun export(onReady: (String) -> Unit) {
        viewModelScope.launch {
            try {
                onReady(access.exportCustomChannels())
            } catch (e: CancellationException) {
                throw e
            } catch (e: ApiException) {
                _state.value = LoadState.Failed(ActionableError.from(e))
            }
        }
    }

    fun import(
        contents: String,
        mode: CustomImportMode,
    ) = mutate {
        access.importCustomChannels(contents, mode)
    }

    private fun mutate(action: suspend () -> Unit) {
        viewModelScope.launch {
            try {
                action()
                load()
            } catch (e: CancellationException) {
                throw e
            } catch (e: ApiException) {
                _state.value = LoadState.Failed(ActionableError.from(e))
            }
        }
    }

    private fun catalog(): CustomCatalog? = (_state.value as? LoadState.Ready)?.value

    companion object {
        fun factory(access: CustomChannelsAccess): ViewModelProvider.Factory =
            viewModelFactory { initializer { CustomChannelsViewModel(access) } }
    }
}
