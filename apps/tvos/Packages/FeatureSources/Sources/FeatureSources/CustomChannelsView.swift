// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import CoreKit
import DesignSystem
import Foundation
import SwiftUI
import core_api

public struct CustomChannelsView: View {
  @State private var model: CustomChannelsModel
  private let onAdd: @MainActor ([CustomGroup]) -> Void
  private let onEdit: @MainActor (CustomChannelSummary, [CustomGroup]) -> Void
  private let onPlay: @MainActor (CustomPlayableChannel) -> Void
  private let onShare: @MainActor () -> Void

  @FocusState private var focused: Focus?
  @State private var groupEditor: GroupEditor?
  @State private var groupName = ""
  @State private var deleteGroupTarget: CustomGroup?
  @State private var deleteChannelTarget: CustomChannelSummary?

  public init(
    access: any CustomChannelsAccess, onAdd: @escaping @MainActor ([CustomGroup]) -> Void,
    onEdit: @escaping @MainActor (CustomChannelSummary, [CustomGroup]) -> Void,
    onPlay: @escaping @MainActor (CustomPlayableChannel) -> Void,
    onShare: @escaping @MainActor () -> Void
  ) {
    _model = State(initialValue: CustomChannelsModel(access: access))
    self.onAdd = onAdd
    self.onEdit = onEdit
    self.onPlay = onPlay
    self.onShare = onShare
  }

  public var body: some View {
    content
      .frame(maxWidth: .infinity, maxHeight: .infinity)
      .background(SpidolaPalette.studio)
      .navigationTitle(String(localized: "Custom channels", bundle: .module))
      .task { await model.load() }
      .sheet(item: $groupEditor) { editor in
        GroupNameSheet(name: $groupName) {
          switch editor {
          case .create: Task { await model.createGroup(name: groupName) }
          case .rename(let group): Task { await model.renameGroup(group, name: groupName) }
          }
        }
      }
      .confirmationDialog(
        String(localized: "Delete group?", bundle: .module),
        isPresented: Binding(
          get: { deleteGroupTarget != nil }, set: { if !$0 { deleteGroupTarget = nil } })
      ) {
        if let group = deleteGroupTarget {
          Button(String(localized: "Delete group", bundle: .module), role: .destructive) {
            Task { await model.deleteGroup(group) }
          }
        }
      } message: {
        Text(String(localized: "Its channels stay in the ungrouped lineup.", bundle: .module))
      }
      .confirmationDialog(
        String(localized: "Delete channel?", bundle: .module),
        isPresented: Binding(
          get: { deleteChannelTarget != nil }, set: { if !$0 { deleteChannelTarget = nil } })
      ) {
        if let channel = deleteChannelTarget {
          Button(String(localized: "Delete channel", bundle: .module), role: .destructive) {
            Task { await model.deleteChannel(channel) }
          }
        }
      }
  }

  @ViewBuilder private var content: some View {
    switch model.state {
    case .loading:
      ProgressView(String(localized: "Loading custom channels…", bundle: .module))
    case .empty:
      catalog(CustomCatalog(groups: [], ungrouped: [], channelsByGroup: [:]))
    case .failed(let error):
      actionableError(
        error, retry: { Task { await model.load() } }, goBack: { Task { await model.load() } })
    case .ready(let content):
      catalog(content)
    }
  }

  private func catalog(_ catalog: CustomCatalog) -> some View {
    ScrollView {
      LazyVStack(alignment: .leading, spacing: SpidolaSpacing.s) {
        controls(catalog.groups)
        channelSection(
          title: String(localized: "Ungrouped", bundle: .module), channels: catalog.ungrouped,
          catalog: catalog)
        ForEach(catalog.groups, id: \.id) { group in
          groupHeader(group, catalog: catalog)
          channelSection(title: nil, channels: catalog.channels(in: group.id), catalog: catalog)
        }
      }
      .padding(.horizontal, SpidolaSpacing.safeHorizontal)
      .padding(.vertical, SpidolaSpacing.safeVertical)
    }
    .onAppear { if focused == nil { focused = .add } }
  }

  private func controls(_ groups: [CustomGroup]) -> some View {
    Group {
      SpidolaRow(
        title: String(localized: "Add channel", bundle: .module), accessory: .symbol("plus"),
        isFocused: focused == .add
      ) { onAdd(groups) }
      .focused($focused, equals: .add)
      .accessibilityIdentifier("custom-add")
      SpidolaRow(
        title: String(localized: "Add group", bundle: .module),
        accessory: .symbol("folder.badge.plus"),
        isFocused: focused == .group
      ) {
        groupName = ""
        groupEditor = .create
      }
      .focused($focused, equals: .group)
      .accessibilityIdentifier("custom-add-group")
      SpidolaRow(
        title: String(localized: "Share or import", bundle: .module),
        accessory: .symbol("arrow.up.arrow.down"), isFocused: focused == .share, action: onShare
      )
      .focused($focused, equals: .share)
      .accessibilityIdentifier("custom-share")
    }
  }

  @ViewBuilder private func channelSection(
    title: String?, channels: [CustomChannelSummary], catalog: CustomCatalog
  ) -> some View {
    if let title {
      Text(title)
        .font(SpidolaType.title)
        .foregroundStyle(SpidolaPalette.broadcastWhite)
        .padding(.top, SpidolaSpacing.l)
    }
    if channels.isEmpty {
      Text(String(localized: "No channels in this group.", bundle: .module))
        .font(SpidolaType.caption)
        .foregroundStyle(SpidolaPalette.staticGray)
        .padding(.vertical, SpidolaSpacing.s)
    } else {
      ForEach(Array(channels.enumerated()), id: \.element.id) { index, channel in
        channelRow(channel, slot: index + 1, channels: channels, catalog: catalog)
      }
    }
  }

  private func channelRow(
    _ channel: CustomChannelSummary, slot: Int, channels: [CustomChannelSummary],
    catalog: CustomCatalog
  ) -> some View {
    Button {
      onPlay(CustomPlayableChannel(channel))
    } label: {
      HStack(spacing: SpidolaSpacing.l) {
        Text(String(format: "%02d", slot))
          .font(SpidolaType.title)
          .foregroundStyle(SpidolaPalette.staticGray)
          .frame(width: 80, alignment: .trailing)
        LogoImage(url: channel.logo)
          .frame(width: 160, height: 90)
          .background(SpidolaPalette.studio)
        VStack(alignment: .leading, spacing: SpidolaSpacing.xs) {
          Text(channel.name)
            .font(SpidolaType.body)
            .foregroundStyle(SpidolaPalette.broadcastWhite)
          if channel.hasUserAgent || channel.headerCount > 0 {
            Text(
              String(
                localized: "\(channel.headerCount) request details", bundle: .module,
                comment: "Custom channel metadata count."
              )
            )
            .font(SpidolaType.caption)
            .foregroundStyle(SpidolaPalette.staticGray)
          }
        }
        Spacer()
        Image(systemName: "ellipsis")
          .foregroundStyle(SpidolaPalette.staticGray)
      }
    }
    .buttonStyle(.plain)
    .padding(SpidolaSpacing.m)
    .background(SpidolaPalette.set)
    .focused($focused, equals: .channel(channel.id))
    .spidolaFocusRing(isFocused: focused == .channel(channel.id))
    .accessibilityLabel(channel.name)
    .accessibilityValue(String(localized: "Position \(slot)", bundle: .module))
    .contextMenu {
      Button(String(localized: "Edit", bundle: .module)) { onEdit(channel, catalog.groups) }
      Button(String(localized: "Move up", bundle: .module)) {
        Task { await model.moveChannelUp(channel) }
      }
      .disabled(channels.first?.id == channel.id)
      Button(String(localized: "Move down", bundle: .module)) {
        Task { await model.moveChannelDown(channel) }
      }
      .disabled(channels.last?.id == channel.id)
      Menu(String(localized: "Move to group", bundle: .module)) {
        if channel.groupId != nil, !catalog.ungrouped.isEmpty {
          Button(String(localized: "Ungrouped", bundle: .module)) {
            Task { await model.moveChannel(channel, to: nil) }
          }
        }
        ForEach(catalog.groups.filter { $0.id != channel.groupId }, id: \.id) { group in
          if !catalog.channels(in: group.id).isEmpty {
            Button(group.name) { Task { await model.moveChannel(channel, to: group.id) } }
          }
        }
      }
      Button(String(localized: "Delete", bundle: .module), role: .destructive) {
        deleteChannelTarget = channel
      }
    }
  }

  private func groupHeader(_ group: CustomGroup, catalog: CustomCatalog) -> some View {
    Text(group.name)
      .font(SpidolaType.title)
      .foregroundStyle(SpidolaPalette.broadcastWhite)
      .padding(.top, SpidolaSpacing.l)
      .focusable()
      .focused($focused, equals: .groupHeader(group.id))
      .contextMenu {
        Button(String(localized: "Rename", bundle: .module)) {
          groupName = group.name
          groupEditor = .rename(group)
        }
        Button(String(localized: "Move up", bundle: .module)) {
          Task { await model.moveGroupUp(group) }
        }
        .disabled(catalog.groups.first?.id == group.id)
        Button(String(localized: "Move down", bundle: .module)) {
          Task { await model.moveGroupDown(group) }
        }
        .disabled(catalog.groups.last?.id == group.id)
        Button(String(localized: "Delete", bundle: .module), role: .destructive) {
          deleteGroupTarget = group
        }
      }
  }

  private enum GroupEditor: Identifiable {
    case create
    case rename(CustomGroup)

    var id: String {
      switch self {
      case .create: "create"
      case .rename(let group): "rename-\(group.id)"
      }
    }
  }

  private enum Focus: Hashable {
    case add, group, share
    case channel(Int64)
    case groupHeader(Int64)
  }
}

private struct GroupNameSheet: View {
  @Binding var name: String
  let onSave: () -> Void
  @Environment(\.dismiss) private var dismiss
  @FocusState private var fieldFocused: Bool

  var body: some View {
    VStack(spacing: SpidolaSpacing.l) {
      Text(String(localized: "Channel group", bundle: .module))
        .font(SpidolaType.title)
        .foregroundStyle(SpidolaPalette.broadcastWhite)
      TextField(String(localized: "Name", bundle: .module), text: $name)
        .font(SpidolaType.body)
        .padding(SpidolaSpacing.m)
        .background(SpidolaPalette.set)
        .focused($fieldFocused)
      HStack(spacing: SpidolaSpacing.m) {
        Button(String(localized: "Cancel", bundle: .module)) { dismiss() }
        Button(String(localized: "Save", bundle: .module)) {
          onSave()
          dismiss()
        }
        .disabled(name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
      }
      .buttonStyle(.plain)
    }
    .padding(SpidolaSpacing.xl)
    .frame(maxWidth: .infinity, maxHeight: .infinity)
    .background(SpidolaPalette.studio)
    .onAppear { fieldFocused = true }
  }
}
