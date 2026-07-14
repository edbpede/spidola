// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import CoreKit
import DesignSystem
import SwiftUI
import core_api

/// The add-source screen: choose URL or paste, enter the details, and watch a live import with a
/// cancel button and a diagnostics summary (PRD §6.1). Xtream accounts and LAN pairing land in
/// Phase 6.
public struct AddSourceView: View {
  @State private var model: AddSourceModel
  private let onFinished: @MainActor () -> Void

  @FocusState private var focused: Field?

  public init(access: any SourcesAccess, onFinished: @escaping @MainActor () -> Void) {
    _model = State(initialValue: AddSourceModel(access: access))
    self.onFinished = onFinished
  }

  public var body: some View {
    content
      .frame(maxWidth: .infinity, maxHeight: .infinity)
      .background(SpidolaPalette.studio)
      .navigationTitle("Add a source")
      .onAppear { model.onFinished = onFinished }
  }

  @ViewBuilder private var content: some View {
    switch model.state {
    case .editing:
      form
    case .importing(let stage, let channels):
      importing(stage: stage, channels: channels)
    case .done(let outcome):
      done(outcome)
    case .failed(let error):
      actionableError(
        error,
        retry: { model.submit() },
        goBack: onFinished,
        fixInput: { model.mode = model.mode })  // returns to the form (state resets on next submit)
    }
  }

  // MARK: - Form

  private var form: some View {
    ScrollView {
      VStack(alignment: .leading, spacing: SpidolaSpacing.m) {
        modePicker
        field("Name", text: $model.name, field: .name)
        switch model.mode {
        case .url:
          field("Playlist URL", text: $model.url, field: .url)
          field("User agent (optional)", text: $model.userAgent, field: .userAgent)
          Toggle("Allow self-signed certificates", isOn: $model.acceptInvalidTls)
            .font(SpidolaType.body)
            .foregroundStyle(SpidolaPalette.broadcastWhite)
            .focused($focused, equals: .tls)
        case .file:
          field("Paste playlist text", text: $model.pastedContent, field: .content)
        }
        if let message = model.validationMessage {
          Text(message)
            .font(SpidolaType.caption)
            .foregroundStyle(SpidolaPalette.streamRed)
        }
        Button("Add source") { model.submit() }
          .buttonStyle(.plain)
          .padding(.horizontal, SpidolaSpacing.l)
          .padding(.vertical, SpidolaSpacing.m)
          .background(SpidolaPalette.testCardAmber)
          .foregroundStyle(SpidolaPalette.studio)
          .font(SpidolaType.body)
          .focused($focused, equals: .submit)
          .spidolaFocusRing(isFocused: focused == .submit)
          .accessibilityIdentifier("add-source-submit")
      }
      .padding(.horizontal, SpidolaSpacing.safeHorizontal)
      .padding(.vertical, SpidolaSpacing.safeVertical)
      .frame(maxWidth: 1100, alignment: .leading)
    }
    .onAppear { focused = .name }
  }

  private var modePicker: some View {
    HStack(spacing: SpidolaSpacing.m) {
      ForEach(AddSourceMode.allCases, id: \.self) { mode in
        Button(mode.title) { model.mode = mode }
          .buttonStyle(.plain)
          .padding(.horizontal, SpidolaSpacing.l)
          .padding(.vertical, SpidolaSpacing.s)
          .background(mode == model.mode ? SpidolaPalette.testCardAmber : SpidolaPalette.set)
          .foregroundStyle(
            mode == model.mode ? SpidolaPalette.studio : SpidolaPalette.broadcastWhite
          )
          .font(SpidolaType.caption)
          .focused($focused, equals: .mode(mode))
          .spidolaFocusRing(isFocused: focused == .mode(mode))
      }
    }
  }

  private func field(_ label: String, text: Binding<String>, field: Field) -> some View {
    TextField(label, text: text)
      .textFieldStyle(.plain)
      .font(SpidolaType.body)
      .foregroundStyle(SpidolaPalette.broadcastWhite)
      .padding(SpidolaSpacing.m)
      .background(SpidolaPalette.set)
      .focused($focused, equals: field)
      .spidolaFocusRing(isFocused: focused == field)
      .accessibilityIdentifier("add-source-\(field)")
  }

  // MARK: - Importing / done

  private func importing(stage: ImportStage, channels: UInt64) -> some View {
    VStack(spacing: SpidolaSpacing.l) {
      ProgressView()
      Text(stageLabel(stage, channels: channels))
        .font(SpidolaType.title)
        .foregroundStyle(SpidolaPalette.broadcastWhite)
      Button("Cancel") { model.cancel() }
        .buttonStyle(.plain)
        .padding(.horizontal, SpidolaSpacing.l)
        .padding(.vertical, SpidolaSpacing.m)
        .background(SpidolaPalette.set)
        .foregroundStyle(SpidolaPalette.broadcastWhite)
        .font(SpidolaType.body)
        .focused($focused, equals: .cancel)
        .spidolaFocusRing(isFocused: focused == .cancel)
        .accessibilityIdentifier("add-source-cancel")
    }
    .frame(maxWidth: .infinity, maxHeight: .infinity)
    .onAppear { focused = .cancel }
  }

  private func done(_ outcome: ImportOutcome) -> some View {
    let skipped = outcome.skipped + outcome.invalid
    return VStack(spacing: SpidolaSpacing.l) {
      Image(systemName: "checkmark.circle.fill")
        .font(.system(size: 56))
        .foregroundStyle(SpidolaPalette.streamGreen)
      Text("Added \(outcome.inserted) channels")
        .font(SpidolaType.title)
        .foregroundStyle(SpidolaPalette.broadcastWhite)
      if skipped > 0 {
        Text("\(skipped) entries were skipped as unreadable.")
          .font(SpidolaType.caption)
          .foregroundStyle(SpidolaPalette.staticGray)
      }
      Button("Done") { onFinished() }
        .buttonStyle(.plain)
        .padding(.horizontal, SpidolaSpacing.l)
        .padding(.vertical, SpidolaSpacing.m)
        .background(SpidolaPalette.testCardAmber)
        .foregroundStyle(SpidolaPalette.studio)
        .font(SpidolaType.body)
        .focused($focused, equals: .done)
        .spidolaFocusRing(isFocused: focused == .done)
        .accessibilityIdentifier("add-source-done")
    }
    .frame(maxWidth: .infinity, maxHeight: .infinity)
    .onAppear { focused = .done }
  }

  private func stageLabel(_ stage: ImportStage, channels: UInt64) -> String {
    switch stage {
    case .connecting: "Connecting…"
    case .downloading: "Importing… \(channels) channels"
    case .finalizing: "Finishing up…"
    @unknown default: "Importing…"
    }
  }

  private enum Field: Hashable {
    case mode(AddSourceMode)
    case name, url, userAgent, content, tls, submit, cancel, done
  }
}
