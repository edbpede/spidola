// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import CoreKit
import DesignSystem
import SwiftUI
import core_api

/// The LAN pairing screen (PRD §6.1): the TV shows an address and a code, someone opens the address
/// on their phone, types the code, and pastes their details into a plain form. What they send
/// pre-fills the add-source screen for the person at the TV to confirm.
///
/// The whole screen is one instruction read left to right — what to open, what to type — with the
/// QR beside it as the shortcut for anyone whose phone can just look at it. Nothing is focusable
/// except the way out: there is nothing here to operate, and inventing focus stops for text would
/// make the remote feel like it does something it does not.
public struct PairingView: View {
  @State private var model: PairingModel
  private let onSubmission: @MainActor (PairingSubmission) -> Void
  private let onCancel: @MainActor () -> Void

  @FocusState private var focused: Bool

  public init(
    access: any PairingAccess,
    onSubmission: @escaping @MainActor (PairingSubmission) -> Void,
    onCancel: @escaping @MainActor () -> Void
  ) {
    _model = State(initialValue: PairingModel(access: access))
    self.onSubmission = onSubmission
    self.onCancel = onCancel
  }

  public var body: some View {
    content
      .frame(maxWidth: .infinity, maxHeight: .infinity)
      .background(SpidolaPalette.studio)
      .navigationTitle(String(localized: "Use my phone", bundle: .module))
      // The server's lifetime is this task's: cancelling it on disappear terminates the event
      // stream, which stops the server (TECH_SPEC §12). Nothing else needs to remember to.
      .task { await model.run() }
      .onDisappear { Task { await model.stop() } }
      .onChange(of: isReceived) { _, received in
        if case .received(let submission) = model.state, received { onSubmission(submission) }
      }
  }

  /// `PairingSubmission` is not `Equatable` for `onChange`'s purposes here — and a `Bool` is the
  /// only thing this needs to notice anyway: the screen hands over exactly once.
  private var isReceived: Bool {
    if case .received = model.state { return true }
    return false
  }

  @ViewBuilder private var content: some View {
    switch model.state {
    case .starting:
      ProgressView(String(localized: "Starting…", bundle: .module))
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    case .waiting(let session):
      waiting(session)
    case .received:
      // Momentary: `onChange` is already navigating to the pre-filled form.
      ProgressView(String(localized: "Got it…", bundle: .module))
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    case .failed(let error):
      actionableError(
        error,
        retry: { Task { await model.retry() } },
        goBack: onCancel,
        fixInput: onCancel)
    }
  }

  private func waiting(_ session: PairingSession) -> some View {
    HStack(alignment: .top, spacing: SpidolaSpacing.xl) {
      VStack(alignment: .leading, spacing: SpidolaSpacing.l) {
        step(
          number: "1", text: String(localized: "On your phone, open this address:", bundle: .module)
        )
        Text(session.url)
          .font(SpidolaType.title)
          .foregroundStyle(SpidolaPalette.testCardAmber)
          .accessibilityLabel(String(localized: "Address", bundle: .module))
          .accessibilityValue(spelledOut(session.url))
        step(number: "2", text: String(localized: "Then type this code:", bundle: .module))
        Text(session.token)
          .font(SpidolaType.display)
          .foregroundStyle(SpidolaPalette.broadcastWhite)
          // Tabular by default across the scale (PRD §8.3), which is what keeps a code from
          // shifting under its own digits as someone reads it aloud.
          .accessibilityLabel(String(localized: "Code", bundle: .module))
          .accessibilityValue(spelledOut(session.token))
        Text(
          String(localized: "Your phone must be on the same network as this TV.", bundle: .module)
        )
        .font(SpidolaType.caption)
        .foregroundStyle(SpidolaPalette.staticGray)
        Spacer(minLength: 0)
        Button(String(localized: "Cancel", bundle: .module)) { onCancel() }
          .buttonStyle(.plain)
          .padding(.horizontal, SpidolaSpacing.l)
          .padding(.vertical, SpidolaSpacing.m)
          .background(SpidolaPalette.set)
          .foregroundStyle(SpidolaPalette.broadcastWhite)
          .font(SpidolaType.body)
          .focused($focused)
          .spidolaFocusRing(isFocused: focused)
          .accessibilityIdentifier("pairing-cancel")
      }
      .frame(maxWidth: .infinity, alignment: .leading)
      QrCode(text: session.url)
    }
    .padding(.horizontal, SpidolaSpacing.safeHorizontal)
    .padding(.vertical, SpidolaSpacing.safeVertical)
    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    .onAppear { focused = true }
  }

  private func step(number: String, text: String) -> some View {
    HStack(spacing: SpidolaSpacing.m) {
      Text(number)
        .font(SpidolaType.caption)
        .foregroundStyle(SpidolaPalette.studio)
        .frame(width: 36, height: 36)
        .background(SpidolaPalette.testCardAmber)
        .clipShape(Circle())
        .accessibilityHidden(true)
      Text(text)
        .font(SpidolaType.body)
        .foregroundStyle(SpidolaPalette.broadcastWhite)
    }
    .accessibilityElement(children: .combine)
  }

  /// Reads an address or a code out character by character.
  ///
  /// VoiceOver says "one ninety-two point one sixty-eight" for `192.168`, which is unusable for
  /// someone copying it onto a phone — and copying it onto a phone is the only thing this screen
  /// asks anyone to do.
  private func spelledOut(_ text: String) -> String {
    text.map(String.init).joined(separator: " ")
  }
}
