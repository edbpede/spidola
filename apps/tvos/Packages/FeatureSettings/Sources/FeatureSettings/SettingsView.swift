// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import CoreKit
import DesignSystem
import SwiftUI
import core_api

/// The settings root (PRD §6.9): one vertical, D-pad-traversable list under section headers, where
/// every row states its setting's name, what it is for, and the value in force. Closed-set rows
/// open a picker; the rest act in place.
///
/// One list and no columns is the whole design. A remote has four directions and settings is a
/// screen people visit rarely and leave quickly — a grid would trade a predictable single axis of
/// travel for cleverness nobody asked for. The quality is meant to come from the focus treatment,
/// the spacing, and the tabular values in the trailing column lining up (PRD §8.1, §8.5): this
/// screen stays quiet so the channel strip can be the one that sings.
public struct SettingsView: View {
  @State private var model: SettingsModel
  private let navigator: SettingsNavigator

  @FocusState private var focused: SettingsRow?
  @State private var isConfirmingClear = false

  public init(access: any SettingsAccess, navigator: SettingsNavigator) {
    _model = State(initialValue: SettingsModel(access: access))
    self.navigator = navigator
  }

  public var body: some View {
    content
      .frame(maxWidth: .infinity, maxHeight: .infinity)
      .background(SpidolaPalette.studio)
      .navigationTitle(String(localized: "Settings", bundle: .module))
      // Re-reads on every appearance, which is also what brings a picker's change back into the
      // rows: `.task` is cancelled when this screen is covered and runs again when it is revealed.
      .task { await model.load() }
      .confirmationDialog(
        String(localized: "Clear history now?", bundle: .module),
        isPresented: $isConfirmingClear,
        titleVisibility: .visible
      ) {
        Button(String(localized: "Clear history", bundle: .module), role: .destructive) {
          Task { await model.clearRecents() }
        }
        Button(String(localized: "Keep it", bundle: .module), role: .cancel) {}
      } message: {
        Text(
          String(
            localized: "Spidola will forget everything you've watched. This can't be undone.",
            bundle: .module))
      }
  }

  @ViewBuilder private var content: some View {
    switch model.state {
    // `.empty` cannot happen: every setting resolves through a core default, so a snapshot always
    // has values. It shares the spinner rather than inventing an empty state no one will see.
    case .loading, .empty:
      ProgressView(String(localized: "Loading…", bundle: .module))
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    case .failed(let error):
      actionableError(
        error,
        retry: { Task { await model.load() } },
        goBack: { Task { await model.load() } })
    case .ready(let settings):
      list(settings)
    }
  }

  private func list(_ settings: AppSettings) -> some View {
    ScrollView {
      VStack(alignment: .leading, spacing: SpidolaSpacing.xl) {
        ForEach(SettingsModel.sections) { section in
          VStack(alignment: .leading, spacing: SpidolaSpacing.s) {
            Text(section.title)
              .font(SpidolaType.title)
              .foregroundStyle(SpidolaPalette.broadcastWhite)
              .padding(.horizontal, SpidolaSpacing.safeHorizontal)
              // The header names the group its rows belong to; VoiceOver announces it as a header
              // so a listener can skim sections the way a sighted viewer skims them.
              .accessibilityAddTraits(.isHeader)
            LazyVStack(spacing: SpidolaSpacing.s) {
              ForEach(section.rows, id: \.self) { row in
                view(for: row, settings)
              }
            }
            .padding(.horizontal, SpidolaSpacing.safeHorizontal)
          }
        }
      }
      .padding(.vertical, SpidolaSpacing.safeVertical)
    }
  }

  @ViewBuilder private func view(for row: SettingsRow, _ settings: AppSettings) -> some View {
    switch row {
    case .choice(let field):
      let value = field.currentValueLabel(in: settings)
      SpidolaRow(
        title: field.title,
        subtitle: field.explanation,
        accessory: .text(value),
        isFocused: focused == row
      ) {
        navigator.openOptions(field)
      }
      .focused($focused, equals: row)
      // A settings row must announce its name *and* the value in force — "Default player,
      // Automatic" — or a listener has to open every picker to learn what anything is set to
      // (PRD §6.10).
      .accessibilityLabel(field.title)
      .accessibilityValue(value)
      .accessibilityHint(field.explanation)
      .accessibilityIdentifier("settings-\(field.rawValue)")

    case .recentsSwitch:
      let value =
        settings.recentsEnabled
        ? String(localized: "On", bundle: .module)
        : String(localized: "Off", bundle: .module)
      SpidolaRow(
        title: String(localized: "Recently watched", bundle: .module),
        subtitle: String(
          localized: "Whether Spidola keeps a list of what you've watched.", bundle: .module),
        accessory: .text(value),
        isFocused: focused == row
      ) {
        Task { await model.setRecentsEnabled(!settings.recentsEnabled) }
      }
      .focused($focused, equals: row)
      .accessibilityLabel(String(localized: "Recently watched", bundle: .module))
      .accessibilityValue(value)
      .accessibilityIdentifier("settings-recents-switch")

    case .clearRecents:
      SpidolaRow(
        title: String(localized: "Clear history now", bundle: .module),
        subtitle: String(
          localized: "Forget everything you've watched so far.", bundle: .module),
        accessory: .symbol("trash"),
        isFocused: focused == row
      ) {
        isConfirmingClear = true
      }
      .focused($focused, equals: row)
      .accessibilityLabel(String(localized: "Clear history now", bundle: .module))
      .accessibilityValue(
        String(
          localized: "Forget everything you've watched so far.",
          bundle: .module)
      )
      .accessibilityIdentifier("settings-clear-recents")

    case .diagnostics:
      SpidolaRow(
        title: String(localized: "Diagnostics", bundle: .module),
        subtitle: String(
          localized: "Versions, and what Spidola has been doing.", bundle: .module),
        accessory: .symbol("chevron.right"),
        isFocused: focused == row
      ) {
        navigator.openDiagnostics()
      }
      .focused($focused, equals: row)
      .accessibilityLabel(String(localized: "Diagnostics", bundle: .module))
      .accessibilityValue(
        String(localized: "Versions, and what Spidola has been doing.", bundle: .module)
      )
      .accessibilityIdentifier("settings-diagnostics")

    case .about:
      SpidolaRow(
        title: String(localized: "About and licenses", bundle: .module),
        subtitle: String(localized: "Spidola and third-party software notices.", bundle: .module),
        accessory: .symbol("info.circle"), isFocused: focused == row
      ) {
        navigator.openAbout()
      }
      .focused($focused, equals: row)
      .accessibilityIdentifier("settings-about")
    }
  }
}
