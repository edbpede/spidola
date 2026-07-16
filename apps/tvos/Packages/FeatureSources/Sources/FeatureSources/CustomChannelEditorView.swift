// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import CoreKit
import DesignSystem
import SwiftUI
import core_api

/// A linear control-room editor. Existing sealed request details are never loaded back into fields.
public struct CustomChannelEditorView: View {
  @State private var model: CustomChannelEditorModel
  private let groups: [CustomGroup]
  private let onFinished: @MainActor () -> Void
  @State private var showRequestDetails = false
  @FocusState private var focused: Field?

  public init(
    summary: CustomChannelSummary?, groups: [CustomGroup], access: any CustomChannelsAccess,
    onFinished: @escaping @MainActor () -> Void
  ) {
    _model = State(initialValue: CustomChannelEditorModel(summary: summary, access: access))
    self.groups = groups
    self.onFinished = onFinished
  }

  public var body: some View {
    ScrollView {
      HStack(alignment: .top, spacing: SpidolaSpacing.xl) {
        form
          .frame(maxWidth: 980)
        preview
          .frame(width: 420)
      }
      .padding(.horizontal, SpidolaSpacing.safeHorizontal)
      .padding(.vertical, SpidolaSpacing.safeVertical)
    }
    .frame(maxWidth: .infinity, maxHeight: .infinity)
    .background(SpidolaPalette.studio)
    .navigationTitle(
      model.isEditing
        ? String(localized: "Edit custom channel", bundle: .module)
        : String(localized: "Add custom channel", bundle: .module)
    )
    .onAppear { focused = .name }
  }

  private var form: some View {
    VStack(alignment: .leading, spacing: SpidolaSpacing.m) {
      if model.isEditing {
        Text(
          String(
            localized:
              "For privacy, saved stream and request details are not shown. Enter them again to save changes.",
            bundle: .module)
        )
        .font(SpidolaType.caption)
        .foregroundStyle(SpidolaPalette.staticGray)
      }
      labeledField(
        String(localized: "Name", bundle: .module), text: $model.input.name, field: .name)
      labeledField(
        String(localized: "Stream address", bundle: .module), text: $model.input.streamAddress,
        field: .address)
      groupPicker
      labeledField(
        String(localized: "Logo address (optional)", bundle: .module), text: $model.input.logo,
        field: .logo)

      Button {
        showRequestDetails.toggle()
      } label: {
        HStack {
          Text(String(localized: "Request details", bundle: .module))
          Spacer()
          Text(
            String(
              localized: "\(configuredDetailCount) configured", bundle: .module,
              comment: "Number of configured custom-channel request details.")
          )
          .foregroundStyle(SpidolaPalette.staticGray)
          Image(systemName: showRequestDetails ? "chevron.up" : "chevron.down")
        }
      }
      .buttonStyle(.plain)
      .font(SpidolaType.body)
      .foregroundStyle(SpidolaPalette.broadcastWhite)
      .padding(SpidolaSpacing.m)
      .background(SpidolaPalette.set)

      if showRequestDetails { requestDetails }

      if let message = model.validationMessage {
        Text(message)
          .font(SpidolaType.caption)
          .foregroundStyle(SpidolaPalette.streamRed)
          .accessibilityIdentifier("custom-validation")
      }

      HStack(spacing: SpidolaSpacing.m) {
        Button(String(localized: "Cancel", bundle: .module), action: onFinished)
          .buttonStyle(.plain)
        Button(String(localized: "Save channel", bundle: .module)) {
          Task { if await model.save() { onFinished() } }
        }
        .buttonStyle(.plain)
        .padding(.horizontal, SpidolaSpacing.l)
        .padding(.vertical, SpidolaSpacing.m)
        .background(SpidolaPalette.testCardAmber)
        .foregroundStyle(SpidolaPalette.studio)
        .disabled(model.isSaving)
        .accessibilityIdentifier("custom-save")
      }
    }
  }

  private var groupPicker: some View {
    VStack(alignment: .leading, spacing: SpidolaSpacing.s) {
      Text(String(localized: "Group", bundle: .module))
        .font(SpidolaType.caption)
        .foregroundStyle(SpidolaPalette.staticGray)
      Picker(
        String(localized: "Group", bundle: .module),
        selection: Binding(get: { model.input.groupId }, set: { model.input.groupId = $0 })
      ) {
        Text(String(localized: "Ungrouped", bundle: .module)).tag(Int64?.none)
        ForEach(groups, id: \.id) { group in Text(group.name).tag(Optional(group.id)) }
      }
      .labelsHidden()
    }
  }

  private var requestDetails: some View {
    VStack(alignment: .leading, spacing: SpidolaSpacing.m) {
      labeledField(
        String(localized: "Player identification (optional)", bundle: .module),
        text: $model.input.userAgent, field: .userAgent)
      ForEach($model.input.headers) { $header in
        HStack(spacing: SpidolaSpacing.m) {
          TextField(String(localized: "Detail name", bundle: .module), text: $header.name)
          SecureField(String(localized: "Detail value", bundle: .module), text: $header.value)
          Button(String(localized: "Remove", bundle: .module)) {
            model.removeHeader(id: header.id)
          }
        }
        .font(SpidolaType.body)
        .padding(SpidolaSpacing.m)
        .background(SpidolaPalette.set)
      }
      Button(String(localized: "Add request detail", bundle: .module)) { model.addHeader() }
    }
  }

  private var preview: some View {
    VStack(alignment: .leading, spacing: SpidolaSpacing.m) {
      Text(String(localized: "Preview", bundle: .module))
        .font(SpidolaType.caption)
        .foregroundStyle(SpidolaPalette.staticGray)
      LogoImage(url: model.input.logo.isEmpty ? nil : model.input.logo)
        .frame(width: 420, height: 236)
        .background(SpidolaPalette.set)
      Text(
        model.input.name.isEmpty
          ? String(localized: "Channel name", bundle: .module) : model.input.name
      )
      .font(SpidolaType.title)
      .foregroundStyle(SpidolaPalette.broadcastWhite)
      .lineLimit(2)
    }
  }

  private var configuredDetailCount: Int {
    model.input.headers.count + (model.input.userAgent.isEmpty ? 0 : 1)
  }

  private func labeledField(
    _ label: String, text: Binding<String>, field: Field
  ) -> some View {
    VStack(alignment: .leading, spacing: SpidolaSpacing.s) {
      Text(label)
        .font(SpidolaType.caption)
        .foregroundStyle(SpidolaPalette.staticGray)
      TextField(label, text: text)
        .textInputAutocapitalization(field == .name ? .words : .never)
        .font(SpidolaType.body)
        .padding(SpidolaSpacing.m)
        .background(SpidolaPalette.set)
        .focused($focused, equals: field)
        .accessibilityLabel(label)
    }
  }

  private enum Field: Hashable { case name, address, logo, userAgent }
}
