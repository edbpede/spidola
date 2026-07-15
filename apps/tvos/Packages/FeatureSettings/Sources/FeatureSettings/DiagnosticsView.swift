// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import CoreKit
import DesignSystem
import SwiftUI

/// The diagnostics screen (PRD §6.9): what Spidola records, what it has recorded lately, and which
/// versions are running.
///
/// Laid out as three fixed bands rather than one long scroll, so the activity pane can own its own
/// scrolling without nesting inside another — nested scroll views fight each other for the D-pad on
/// tvOS, and the pane is the only thing here long enough to need scrolling at all.
public struct DiagnosticsView: View {
  @State private var model: DiagnosticsModel
  private let navigator: SettingsNavigator

  @FocusState private var focused: Focus?

  public init(access: any SettingsAccess, navigator: SettingsNavigator) {
    _model = State(initialValue: DiagnosticsModel(access: access))
    self.navigator = navigator
  }

  public var body: some View {
    content
      .frame(maxWidth: .infinity, maxHeight: .infinity)
      .background(SpidolaPalette.studio)
      .navigationTitle(String(localized: "Diagnostics", bundle: .module))
      // Runs again when the log-level picker is popped, so the level row and the activity below it
      // both catch up in one read.
      .task { await model.load() }
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
        goBack: { Task { await model.load() } })
    case .ready(let content):
      bands(content)
    }
  }

  private func bands(_ content: DiagnosticsContent) -> some View {
    VStack(alignment: .leading, spacing: SpidolaSpacing.l) {
      logLevelRow(content)
      activity(content)
      versions(content)
    }
    .padding(.horizontal, SpidolaSpacing.safeHorizontal)
    .padding(.vertical, SpidolaSpacing.safeVertical)
    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
  }

  private func logLevelRow(_ content: DiagnosticsContent) -> some View {
    let value = LogLevelOption.from(content.logLevel).label
    return VStack(alignment: .leading, spacing: SpidolaSpacing.s) {
      header(String(localized: "Recording", bundle: .module))
      SpidolaRow(
        title: SettingsField.logLevel.title,
        subtitle: SettingsField.logLevel.explanation,
        accessory: .text(value),
        isFocused: focused == .logLevel
      ) {
        navigator.openOptions(.logLevel)
      }
      .focused($focused, equals: .logLevel)
      .accessibilityLabel(SettingsField.logLevel.title)
      .accessibilityValue(value)
      .accessibilityHint(SettingsField.logLevel.explanation)
      .accessibilityIdentifier("diagnostics-log-level")
    }
  }

  /// The log ring, on screen. This *is* export on tvOS — see `DiagnosticsModel` for why a file
  /// would be worse than useless here.
  private func activity(_ content: DiagnosticsContent) -> some View {
    VStack(alignment: .leading, spacing: SpidolaSpacing.s) {
      header(String(localized: "Recent activity", bundle: .module))
      Group {
        if content.recentActivity.isEmpty {
          Text(String(localized: "Nothing recorded yet.", bundle: .module))
            .font(SpidolaType.caption)
            .foregroundStyle(SpidolaPalette.staticGray)
            .padding(SpidolaSpacing.m)
            .frame(maxWidth: .infinity, alignment: .leading)
        } else {
          logPane(content.recentActivity)
        }
      }
      .background(SpidolaPalette.set)
      .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
    }
  }

  private func logPane(_ lines: [String]) -> some View {
    ScrollView {
      VStack(alignment: .leading, spacing: SpidolaSpacing.xs) {
        ForEach(Array(lines.enumerated()), id: \.offset) { _, line in
          // No `.textSelection` here: tvOS has no pointer and no pasteboard to select into, so
          // the line is read from the screen or photographed. That is also why the pane is
          // scrollable rather than truncated — the whole line has to be legible.
          Text(line)
            .font(SpidolaType.caption)
            .foregroundStyle(SpidolaPalette.broadcastWhite)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
      }
      .padding(SpidolaSpacing.m)
    }
    .frame(maxWidth: .infinity, maxHeight: 420, alignment: .leading)
    // One focus stop that scrolls, rather than one stop per line: a log ring is hundreds of rows,
    // and making each a focus target would bury every control below it behind hundreds of clicks.
    .focusable(true)
    .focused($focused, equals: .activity)
    .spidolaFocusRing(isFocused: focused == .activity)
    .accessibilityLabel(String(localized: "Recent activity", bundle: .module))
    // VoiceOver reads the value, so the lines have to be *in* it: a pane that announced only its
    // name would tell a blind user a log exists and never let them hear it.
    .accessibilityValue(lines.joined(separator: ". "))
    .accessibilityIdentifier("diagnostics-activity")
  }

  private func versions(_ content: DiagnosticsContent) -> some View {
    VStack(alignment: .leading, spacing: SpidolaSpacing.s) {
      header(String(localized: "Versions", bundle: .module))
      // A grid so the values form a column: every face in the scale is already tabular
      // (`SpidolaType`), so the digits line up down the page rather than shimmering (PRD §8.3).
      Grid(
        alignment: .leading, horizontalSpacing: SpidolaSpacing.l,
        verticalSpacing: SpidolaSpacing.s
      ) {  // swiftlint:disable:this opening_brace
        ForEach(content.versions) { fact in
          GridRow {
            Text(fact.label)
              .font(SpidolaType.caption)
              .foregroundStyle(SpidolaPalette.staticGray)
            Text(fact.value)
              .font(SpidolaType.caption)
              .foregroundStyle(SpidolaPalette.broadcastWhite)
          }
          .accessibilityElement(children: .combine)
          .accessibilityLabel(fact.label)
          .accessibilityValue(fact.value)
        }
      }
      .padding(SpidolaSpacing.m)
      .frame(maxWidth: .infinity, alignment: .leading)
      .background(SpidolaPalette.set)
      .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
    }
  }

  private func header(_ title: String) -> some View {
    Text(title)
      .font(SpidolaType.title)
      .foregroundStyle(SpidolaPalette.broadcastWhite)
      .accessibilityAddTraits(.isHeader)
  }

  private enum Focus: Hashable {
    case logLevel
    case activity
  }
}
