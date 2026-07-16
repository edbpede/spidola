// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import CoreKit
import DesignSystem
import SwiftUI
import core_api

/// The add-source screen: choose a playlist URL, pasted text, or an Xtream account, enter the
/// details, and watch a live import with a cancel button and a diagnostics summary (PRD §6.1).
///
/// It is also where a phone's pairing submission lands, pre-filled and waiting to be confirmed —
/// the same screen and the same "Add source" button, because a submission is an input method, not
/// a fourth kind of source.
public struct AddSourceView: View {
  @State private var model: AddSourceModel
  private let onFinished: @MainActor () -> Void

  @FocusState private var focused: Field?

  /// - Parameter prefill: what a phone sent, if this screen was reached through pairing. Applied
  ///   once, when the form is first built; the person at the TV confirms or edits it.
  public init(
    access: any SourcesAccess,
    prefill: PairingSubmission? = nil,
    onFinished: @escaping @MainActor () -> Void
  ) {
    let model = AddSourceModel(access: access)
    if let prefill { model.prefill(from: prefill) }
    _model = State(initialValue: model)
    self.onFinished = onFinished
  }

  public var body: some View {
    content
      .frame(maxWidth: .infinity, maxHeight: .infinity)
      .background(SpidolaPalette.studio)
      .navigationTitle(String(localized: "Add a source", bundle: .module))
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
        // `Unauthorized` prescribes `fixInput`, and a rejected Xtream password is the likeliest
        // failure this screen has — so "Edit" must actually put the fields back on screen.
        fixInput: { model.returnToForm() })
    }
  }

  // MARK: - Form

  private var form: some View {
    ScrollView {
      VStack(alignment: .leading, spacing: SpidolaSpacing.m) {
        modePicker
        field(String(localized: "Name", bundle: .module), text: $model.name, field: .name)
        switch model.mode {
        case .url:
          field(String(localized: "Playlist URL", bundle: .module), text: $model.url, field: .url)
          field(
            String(localized: "User agent (optional)", bundle: .module),
            text: $model.userAgent, field: .userAgent)
          Toggle(
            String(localized: "Allow self-signed certificates", bundle: .module),
            isOn: $model.acceptInvalidTls
          )
          .font(SpidolaType.body)
          .foregroundStyle(SpidolaPalette.broadcastWhite)
          .focused($focused, equals: .tls)
        case .file:
          field(
            String(localized: "Paste playlist text", bundle: .module),
            text: $model.pastedContent, field: .content)
        case .xtream:
          field(
            String(localized: "Server address", bundle: .module),
            text: $model.server, field: .server)
          field(
            String(localized: "Username", bundle: .module),
            text: $model.username, field: .username)
          secureField(
            String(localized: "Password", bundle: .module),
            text: $model.password, field: .password)
          Text(
            String(
              localized: "Spidola checks these with your provider before saving them.",
              bundle: .module)
          )
          .font(SpidolaType.caption)
          .foregroundStyle(SpidolaPalette.staticGray)
        }
        if let validation = model.validation {
          Text(validation.message)
            .font(SpidolaType.caption)
            .foregroundStyle(SpidolaPalette.streamRed)
        }
        Button(String(localized: "Add source", bundle: .module)) { model.submit() }
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

  /// A masked field. `SecureField` is what keeps a password off a living-room screen — the one
  /// place in this app where someone else is quite likely to be watching.
  private func secureField(_ label: String, text: Binding<String>, field: Field) -> some View {
    SecureField(label, text: text)
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
      Button(String(localized: "Cancel", bundle: .module)) { model.cancel() }
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
        .accessibilityHidden(true)
      // Widened to `Int` before interpolating so the extracted keys are plain `%lld`, and
      // pluralised through the catalog: an import of exactly one channel is ordinary, and
      // "Added 1 channels" is the kind of seam that makes an app feel machine-made.
      Text(String(localized: "Added \(Int(outcome.inserted)) channels", bundle: .module))
        .font(SpidolaType.title)
        .foregroundStyle(SpidolaPalette.broadcastWhite)
      if skipped > 0 {
        Text(
          String(
            localized: "\(Int(skipped)) entries were skipped as unreadable.", bundle: .module)
        )
        .font(SpidolaType.caption)
        .foregroundStyle(SpidolaPalette.staticGray)
      }
      Button(String(localized: "Done", bundle: .module)) { onFinished() }
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
    case .connecting: String(localized: "Connecting…", bundle: .module)
    case .downloading:
      String(localized: "Importing… \(Int(channels)) channels", bundle: .module)
    case .finalizing: String(localized: "Finishing up…", bundle: .module)
    @unknown default: String(localized: "Importing…", bundle: .module)
    }
  }

  private enum Field: Hashable {
    case mode(AddSourceMode)
    case name, url, userAgent, content, tls, submit, cancel, done
    case server, username, password
  }
}
