// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import SwiftUI

/// A trailing accessory on a `SpidolaRow` — a status word or an SF Symbol (e.g. a favorite star).
///
/// The two are not equivalent to a listener. A `.text` accessory is words and is read out with the
/// row; a `.symbol` reads as its symbol name — "trash", "star fill" — which is never what the row
/// means, so it is kept out of the announcement entirely. Any state a glyph stands for therefore
/// has to be given words by the caller, as an `.accessibilityValue` on the row.
public enum RowAccessory: Sendable, Equatable {
  case none
  case text(String)
  case symbol(String)
}

/// A full-width, D-pad-focusable list row: a title, an optional subtitle, and an optional trailing
/// accessory, wearing the Test-Card Amber focus treatment. Focus is owned by the caller (via
/// `@FocusState`) and passed in, so the row rides the native focus engine rather than fighting it
/// (PRD §8.4). Used across the sources, groups, and channel lists.
public struct SpidolaRow: View {
  private let title: String
  private let subtitle: String?
  private let accessory: RowAccessory
  private let isFocused: Bool
  private let action: () -> Void

  public init(
    title: String,
    subtitle: String? = nil,
    accessory: RowAccessory = .none,
    isFocused: Bool,
    action: @escaping () -> Void
  ) {
    self.title = title
    self.subtitle = subtitle
    self.accessory = accessory
    self.isFocused = isFocused
    self.action = action
  }

  public var body: some View {
    Button(action: action) {
      HStack(spacing: SpidolaSpacing.m) {
        VStack(alignment: .leading, spacing: SpidolaSpacing.xs) {
          Text(title)
            .font(SpidolaType.body)
            .foregroundStyle(SpidolaPalette.broadcastWhite)
            .lineLimit(1)
          if let subtitle {
            Text(subtitle)
              .font(SpidolaType.caption)
              .foregroundStyle(SpidolaPalette.staticGray)
              .lineLimit(1)
          }
        }
        Spacer(minLength: SpidolaSpacing.m)
        accessoryView
      }
      .frame(maxWidth: .infinity, alignment: .leading)
      .padding(SpidolaSpacing.m)
      .background(SpidolaPalette.set)
    }
    .buttonStyle(.plain)
    .spidolaFocusRing(isFocused: isFocused)
  }

  @ViewBuilder private var accessoryView: some View {
    switch accessory {
    case .none:
      EmptyView()
    case .text(let value):
      Text(value)
        .font(SpidolaType.caption)
        .foregroundStyle(SpidolaPalette.staticGray)
    case .symbol(let name):
      Image(systemName: name)
        .font(.system(size: 26))
        .foregroundStyle(SpidolaPalette.testCardAmber)
        .accessibilityHidden(true)
    }
  }
}
