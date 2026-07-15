// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import CoreKit
import DesignSystem
import SwiftUI

/// The one picker screen in this slice: the values a closed-set setting can take, with the one in
/// force marked, above a short line saying what each does. Picking writes it and closes.
///
/// One screen serves every closed-set setting because a picker has nothing setting-specific in it —
/// `SettingsField` answers what to title it, what to offer, what is picked, and how to write it
/// back. A per-setting screen would be nine copies of this file drifting apart.
public struct SettingsOptionsView: View {
  @State private var model: SettingsOptionsModel
  private let onFinished: @MainActor () -> Void

  @FocusState private var focused: String?

  public init(
    field: SettingsField,
    access: any SettingsAccess,
    onFinished: @escaping @MainActor () -> Void
  ) {
    _model = State(initialValue: SettingsOptionsModel(field: field, access: access))
    self.onFinished = onFinished
  }

  public var body: some View {
    content
      .frame(maxWidth: .infinity, maxHeight: .infinity)
      .background(SpidolaPalette.studio)
      .navigationTitle(model.field.title)
      .task {
        await model.load()
        // Open on the value in force, so the viewer starts where they are rather than at the top
        // of a list they then have to read to find themselves in.
        focused = model.selectedChoiceId ?? model.choices.first?.id
      }
  }

  @ViewBuilder private var content: some View {
    switch model.state {
    case .loading, .empty:
      ProgressView(String(localized: "Loading…", bundle: .module))
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    case .failed(let error):
      actionableError(
        error,
        retry: { Task { await model.load() } },
        goBack: onFinished)
    case .ready:
      list
    }
  }

  private var list: some View {
    ScrollView {
      VStack(alignment: .leading, spacing: SpidolaSpacing.s) {
        Text(model.field.explanation)
          .font(SpidolaType.body)
          .foregroundStyle(SpidolaPalette.staticGray)
          .padding(.horizontal, SpidolaSpacing.safeHorizontal)
          .padding(.bottom, SpidolaSpacing.m)
        LazyVStack(spacing: SpidolaSpacing.s) {
          ForEach(model.choices) { choice in
            row(choice)
          }
        }
        .padding(.horizontal, SpidolaSpacing.safeHorizontal)
      }
      .padding(.vertical, SpidolaSpacing.safeVertical)
    }
  }

  private func row(_ choice: SettingsChoice) -> some View {
    let isSelected = model.selectedChoiceId == choice.id
    return SpidolaRow(
      title: choice.label,
      subtitle: choice.detail,
      accessory: isSelected ? .symbol("checkmark") : .none,
      isFocused: focused == choice.id
    ) {
      Task {
        // Only leave once the write has actually landed; a failure keeps the picker up with an
        // actionable error rather than returning to rows that would show the old value as if
        // nothing had been asked for.
        if await model.choose(choice.id) { onFinished() }
      }
    }
    .focused($focused, equals: choice.id)
    // The checkmark is the only thing marking the current value, and it is invisible to a screen
    // reader unless the row says so in words.
    .accessibilityLabel(choice.label)
    .accessibilityValue(
      isSelected
        ? String(localized: "Selected", bundle: .module)
        : String(localized: "Not selected", bundle: .module)
    )
    .accessibilityIdentifier("option-\(choice.id)")
  }
}
