// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import CoreKit
import DesignSystem
import SwiftUI
import core_api

/// A source and the fields the row needs — kept `Identifiable` for the rename/delete sheets.
private struct SourceTarget: Identifiable {
  let id: Int64
  let name: String
}

/// The manage-sources screen: add a source, then rename / enable-disable / refresh / delete / set
/// auto-refresh on each (PRD §6.1). Each source's actions live in its context menu; refresh streams
/// through the core and preserves favorites and hidden flags (§4.4).
public struct SourcesView: View {
  @State private var model: SourcesModel
  private let onAddSource: @MainActor () -> Void
  private let onPair: @MainActor () -> Void

  @FocusState private var focused: Focus?
  @State private var renameTarget: SourceTarget?
  @State private var autoRefreshTarget: SourceTarget?
  @State private var deleteTarget: SourceTarget?
  @State private var renameText = ""

  public init(
    access: any SourcesAccess,
    onAddSource: @escaping @MainActor () -> Void,
    onPair: @escaping @MainActor () -> Void
  ) {
    _model = State(initialValue: SourcesModel(access: access))
    self.onAddSource = onAddSource
    self.onPair = onPair
  }

  public var body: some View {
    content
      .frame(maxWidth: .infinity, maxHeight: .infinity)
      .background(SpidolaPalette.studio)
      .navigationTitle(String(localized: "Sources", bundle: .module))
      .task { await model.load() }
      .sheet(item: $renameTarget) { target in
        RenameSheet(name: $renameText) { newName in
          Task { await model.rename(id: target.id, to: newName) }
        }
      }
      .confirmationDialog(
        String(localized: "Auto-refresh", bundle: .module),
        isPresented: autoRefreshBinding,
        titleVisibility: .visible
      ) {
        if let target = autoRefreshTarget {
          ForEach(AutoRefreshOption.allCases) { option in
            Button(option.label) {
              Task { await model.setAutoRefresh(id: target.id, option: option) }
            }
          }
        }
      }
      .confirmationDialog(
        String(localized: "Delete source?", bundle: .module),
        isPresented: deleteBinding,
        titleVisibility: .visible
      ) {
        if let target = deleteTarget {
          Button(String(localized: "Delete \(target.name)", bundle: .module), role: .destructive) {
            Task { await model.delete(id: target.id) }
          }
          Button(String(localized: "Cancel", bundle: .module), role: .cancel) {}
        }
      } message: {
        Text(
          String(
            localized: "Its channels, favorites, and history are removed. This can't be undone.",
            bundle: .module))
      }
  }

  @ViewBuilder private var content: some View {
    switch model.state {
    case .loading:
      ProgressView(String(localized: "Loading sources…", bundle: .module))
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    case .failed(let error):
      actionableError(error, retry: { Task { await model.load() } }, goBack: onAddSource)
    case .empty:
      list(sources: [])
    case .ready(let sources):
      list(sources: sources)
    }
  }

  private func list(sources: [Source]) -> some View {
    ScrollView {
      VStack(alignment: .leading, spacing: SpidolaSpacing.s) {
        if let status = model.statusMessage {
          Text(status)
            .font(SpidolaType.caption)
            .foregroundStyle(SpidolaPalette.testCardAmber)
            .padding(.horizontal, SpidolaSpacing.safeHorizontal)
        }
        SpidolaRow(
          title: String(localized: "Add a source", bundle: .module),
          accessory: .symbol("plus"),
          isFocused: focused == .add
        ) {
          onAddSource()
        }
        .focused($focused, equals: .add)
        .accessibilityIdentifier("sources-add")

        SpidolaRow(
          title: String(localized: "Use my phone", bundle: .module),
          subtitle: String(
            localized: "Send a playlist or account from your phone instead of typing it here.",
            bundle: .module),
          accessory: .symbol("iphone"),
          isFocused: focused == .pair
        ) {
          onPair()
        }
        .focused($focused, equals: .pair)
        .accessibilityLabel(String(localized: "Use my phone", bundle: .module))
        .accessibilityValue(
          String(
            localized: "Send a playlist or account from your phone instead of typing it here.",
            bundle: .module)
        )
        .accessibilityIdentifier("sources-pair")

        if sources.isEmpty {
          Text(String(localized: "No sources yet — add one to start watching.", bundle: .module))
            .font(SpidolaType.body)
            .foregroundStyle(SpidolaPalette.staticGray)
            .padding(SpidolaSpacing.m)
        }

        ForEach(sources, id: \.id) { source in
          sourceRow(source)
        }
      }
      .padding(.horizontal, SpidolaSpacing.safeHorizontal)
      .padding(.vertical, SpidolaSpacing.safeVertical)
    }
  }

  private func sourceRow(_ source: Source) -> some View {
    SpidolaRow(
      title: source.name,
      subtitle: subtitle(for: source),
      accessory: model.refreshingIds.contains(source.id)
        ? .text(String(localized: "Refreshing…", bundle: .module))
        : (source.common.enabled
          ? .none : .text(String(localized: "Disabled", bundle: .module))),
      isFocused: focused == .source(source.id)
    ) {
      // Selecting a source opens its actions; the context menu holds the same set.
    }
    .focused($focused, equals: .source(source.id))
    .accessibilityIdentifier("manage-source-\(source.name)")
    .contextMenu {
      Button(String(localized: "Rename", bundle: .module)) {
        renameText = source.name
        renameTarget = SourceTarget(id: source.id, name: source.name)
      }
      Button(
        source.common.enabled
          ? String(localized: "Disable", bundle: .module)
          : String(localized: "Enable", bundle: .module)
      ) {
        Task { await model.setEnabled(id: source.id, enabled: !source.common.enabled) }
      }
      if source.isRefreshable {
        Button(String(localized: "Refresh now", bundle: .module)) {
          Task { await model.refresh(source) }
        }
        Button(String(localized: "Auto-refresh…", bundle: .module)) {
          autoRefreshTarget = SourceTarget(id: source.id, name: source.name)
        }
      }
      Button(String(localized: "Delete", bundle: .module), role: .destructive) {
        deleteTarget = SourceTarget(id: source.id, name: source.name)
      }
    }
  }

  private func subtitle(for source: Source) -> String {
    let refresh = AutoRefreshOption.from(seconds: source.common.autoRefreshSecs).label
    return "\(source.kindLabel) · \(refresh)"
  }

  private var autoRefreshBinding: Binding<Bool> {
    Binding(get: { autoRefreshTarget != nil }, set: { if !$0 { autoRefreshTarget = nil } })
  }

  private var deleteBinding: Binding<Bool> {
    Binding(get: { deleteTarget != nil }, set: { if !$0 { deleteTarget = nil } })
  }

  private enum Focus: Hashable {
    case add
    case pair
    case source(Int64)
  }
}

/// A minimal rename sheet — a single text field and Save/Cancel. A sheet (not an alert text field)
/// because alert text entry is unreliable on tvOS.
private struct RenameSheet: View {
  @Binding var name: String
  let onSave: (String) -> Void
  @Environment(\.dismiss) private var dismiss
  @FocusState private var fieldFocused: Bool

  var body: some View {
    VStack(spacing: SpidolaSpacing.l) {
      Text(String(localized: "Rename source", bundle: .module))
        .font(SpidolaType.title)
        .foregroundStyle(SpidolaPalette.broadcastWhite)
      TextField(String(localized: "Name", bundle: .module), text: $name)
        .textFieldStyle(.plain)
        .font(SpidolaType.body)
        .foregroundStyle(SpidolaPalette.broadcastWhite)
        .padding(SpidolaSpacing.m)
        .background(SpidolaPalette.set)
        .focused($fieldFocused)
      HStack(spacing: SpidolaSpacing.m) {
        Button(String(localized: "Cancel", bundle: .module)) { dismiss() }
          .buttonStyle(.plain)
        Button(String(localized: "Save", bundle: .module)) {
          onSave(name)
          dismiss()
        }
        .buttonStyle(.plain)
        .padding(.horizontal, SpidolaSpacing.l)
        .padding(.vertical, SpidolaSpacing.s)
        .background(SpidolaPalette.testCardAmber)
        .foregroundStyle(SpidolaPalette.studio)
      }
    }
    .padding(SpidolaSpacing.xl)
    .frame(maxWidth: .infinity, maxHeight: .infinity)
    .background(SpidolaPalette.studio)
    .onAppear { fieldFocused = true }
  }
}
