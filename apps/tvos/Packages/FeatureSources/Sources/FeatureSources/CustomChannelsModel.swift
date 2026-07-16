// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import CoreKit
import Observation
import core_api

public struct CustomCatalog: Sendable {
  public let groups: [CustomGroup]
  public let ungrouped: [CustomChannelSummary]
  public let channelsByGroup: [Int64: [CustomChannelSummary]]

  public func channels(in groupId: Int64?) -> [CustomChannelSummary] {
    guard let groupId else { return ungrouped }
    return channelsByGroup[groupId] ?? []
  }
}

@MainActor
@Observable
public final class CustomChannelsModel {
  public private(set) var state: LoadState<CustomCatalog> = .loading

  private let access: any CustomChannelsAccess

  public init(access: any CustomChannelsAccess) {
    self.access = access
  }

  public func load() async {
    state = .loading
    do {
      let groups = try await access.customGroups()
      let ungrouped = try await access.customChannels(groupId: nil)
      var grouped: [Int64: [CustomChannelSummary]] = [:]
      for group in groups {
        grouped[group.id] = try await access.customChannels(groupId: group.id)
      }
      state = .ready(
        CustomCatalog(groups: groups, ungrouped: ungrouped, channelsByGroup: grouped))
    } catch {
      if let failed = LoadState<CustomCatalog>.failure(from: error) { state = failed }
    }
  }

  public func createGroup(name: String) async {
    await mutate { _ = try await access.createCustomGroup(name: name) }
  }

  public func renameGroup(_ group: CustomGroup, name: String) async {
    await mutate { try await access.renameCustomGroup(id: group.id, name: name) }
  }

  public func deleteGroup(_ group: CustomGroup) async {
    await mutate { try await access.deleteCustomGroup(id: group.id) }
  }

  public func moveGroupUp(_ group: CustomGroup) async {
    guard case .ready(let catalog) = state,
      let index = catalog.groups.firstIndex(of: group), index > catalog.groups.startIndex
    else { return }
    await mutate {
      try await access.moveCustomGroupBefore(id: group.id, anchorId: catalog.groups[index - 1].id)
    }
  }

  public func moveGroupDown(_ group: CustomGroup) async {
    guard case .ready(let catalog) = state,
      let index = catalog.groups.firstIndex(of: group),
      index < catalog.groups.index(before: catalog.groups.endIndex)
    else { return }
    await mutate {
      try await access.moveCustomGroupAfter(id: group.id, anchorId: catalog.groups[index + 1].id)
    }
  }

  public func deleteChannel(_ channel: CustomChannelSummary) async {
    await mutate { try await access.deleteCustomChannel(id: channel.id) }
  }

  public func moveChannelUp(_ channel: CustomChannelSummary) async {
    guard case .ready(let catalog) = state else { return }
    let channels = catalog.channels(in: channel.groupId)
    guard let index = channels.firstIndex(of: channel), index > channels.startIndex else { return }
    await mutate {
      try await access.moveCustomChannelBefore(id: channel.id, anchorId: channels[index - 1].id)
    }
  }

  public func moveChannelDown(_ channel: CustomChannelSummary) async {
    guard case .ready(let catalog) = state else { return }
    let channels = catalog.channels(in: channel.groupId)
    guard let index = channels.firstIndex(of: channel),
      index < channels.index(before: channels.endIndex)
    else { return }
    await mutate {
      try await access.moveCustomChannelAfter(id: channel.id, anchorId: channels[index + 1].id)
    }
  }

  /// Moves across groups by anchoring before the target group's first channel. Empty groups have no
  /// legal anchor in the bounded core API and are intentionally omitted by the view.
  public func moveChannel(_ channel: CustomChannelSummary, to groupId: Int64?) async {
    guard case .ready(let catalog) = state,
      let anchor = catalog.channels(in: groupId).first,
      anchor.id != channel.id
    else { return }
    await mutate { try await access.moveCustomChannelBefore(id: channel.id, anchorId: anchor.id) }
  }

  private func mutate(_ action: () async throws -> Void) async {
    do {
      try await action()
      await load()
    } catch {
      if let failed = LoadState<CustomCatalog>.failure(from: error) { state = failed }
    }
  }
}
