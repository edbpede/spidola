// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import SwiftUI

/// One labelled action button for an actionable error.
public struct SpidolaErrorButton: Identifiable, Sendable {
  public let id = UUID()
  public let title: String
  public let action: @MainActor () -> Void

  public init(title: String, action: @escaping @MainActor () -> Void) {
    self.title = title
    self.action = action
  }
}

/// Presents a failure with its plain-language class, a one-sentence message, and a **non-empty**
/// set of actions (PRD §6.3). "No action available" is unrepresentable: `primary` is a single
/// required button, so the view always offers at least one thing to do — an error dead-end can
/// never be rendered.
public struct ActionableErrorView: View {
  private let failureClass: String
  private let message: String
  private let primary: SpidolaErrorButton
  private let others: [SpidolaErrorButton]

  @FocusState private var focused: UUID?

  public init(
    failureClass: String,
    message: String,
    primary: SpidolaErrorButton,
    others: [SpidolaErrorButton] = []
  ) {
    self.failureClass = failureClass
    self.message = message
    self.primary = primary
    self.others = others
  }

  private var buttons: [SpidolaErrorButton] { [primary] + others }

  public var body: some View {
    VStack(spacing: SpidolaSpacing.l) {
      Image(systemName: "exclamationmark.triangle.fill")
        .font(.system(size: 56))
        .foregroundStyle(SpidolaPalette.streamRed)
        .accessibilityHidden(true)
      Text(failureClass)
        .font(SpidolaType.title)
        .foregroundStyle(SpidolaPalette.broadcastWhite)
        .multilineTextAlignment(.center)
      Text(message)
        .font(SpidolaType.body)
        .foregroundStyle(SpidolaPalette.staticGray)
        .multilineTextAlignment(.center)
        .frame(maxWidth: 900)
      HStack(spacing: SpidolaSpacing.m) {
        ForEach(buttons) { button in
          Button(button.title, action: button.action)
            .buttonStyle(.plain)
            .padding(.horizontal, SpidolaSpacing.l)
            .padding(.vertical, SpidolaSpacing.m)
            .background(
              button.id == primary.id ? SpidolaPalette.testCardAmber : SpidolaPalette.set
            )
            .foregroundStyle(
              button.id == primary.id ? SpidolaPalette.studio : SpidolaPalette.broadcastWhite
            )
            .font(SpidolaType.body)
            .focused($focused, equals: button.id)
            .spidolaFocusRing(isFocused: focused == button.id)
        }
      }
      .padding(.top, SpidolaSpacing.s)
    }
    .padding(SpidolaSpacing.xl)
    .frame(maxWidth: .infinity, maxHeight: .infinity)
    .background(SpidolaPalette.studio)
    .onAppear { focused = primary.id }
    .accessibilityElement(children: .contain)
    // The words are the caller's; only the punctuation joining them is this layer's, so the format
    // goes through the catalog and a language that wants the halves the other way round can say so.
    .accessibilityLabel(String(localized: "\(failureClass). \(message)", bundle: .module))
  }
}
