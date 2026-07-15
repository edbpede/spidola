// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import SwiftUI

/// One card in a `PosterRail`. `id` is a caller-composed stable key (e.g. source+identity) so it
/// doubles as the `ForEach` identity and the focus key across a data refresh.
public struct PosterItem: Identifiable, Hashable, Sendable {
  public let id: String
  public let title: String
  public let subtitle: String?
  public let logo: String?

  public init(id: String, title: String, subtitle: String? = nil, logo: String? = nil) {
    self.id = id
    self.title = title
    self.subtitle = subtitle
    self.logo = logo
  }
}

/// A titled, horizontally-scrolling rail of poster cards — the home screen's favorites and recents
/// rows (PRD §8.3). D-pad focus moves card to card, each wearing the Test-Card Amber treatment;
/// the native focus engine handles pivot scrolling. Empty rails render nothing so the caller can
/// omit the section entirely.
public struct PosterRail: View {
  private let title: String
  private let items: [PosterItem]
  private let onSelect: (PosterItem) -> Void

  @FocusState private var focused: String?

  public init(title: String, items: [PosterItem], onSelect: @escaping (PosterItem) -> Void) {
    self.title = title
    self.items = items
    self.onSelect = onSelect
  }

  public var body: some View {
    if items.isEmpty {
      EmptyView()
    } else {
      VStack(alignment: .leading, spacing: SpidolaSpacing.s) {
        Text(title)
          .font(SpidolaType.title)
          .foregroundStyle(SpidolaPalette.broadcastWhite)
          .padding(.horizontal, SpidolaSpacing.safeHorizontal)
        ScrollView(.horizontal, showsIndicators: false) {
          LazyHStack(spacing: SpidolaSpacing.m) {
            ForEach(items) { item in
              PosterCard(item: item, isFocused: focused == item.id) { onSelect(item) }
                .focused($focused, equals: item.id)
            }
          }
          .padding(.horizontal, SpidolaSpacing.safeHorizontal)
          .padding(.vertical, SpidolaSpacing.s)
        }
      }
    }
  }
}

private struct PosterCard: View {
  let item: PosterItem
  let isFocused: Bool
  let action: () -> Void

  private let cardWidth: CGFloat = 240

  var body: some View {
    Button(action: action) {
      VStack(alignment: .leading, spacing: SpidolaSpacing.xs) {
        LogoImage(url: item.logo)
          .frame(width: cardWidth, height: cardWidth * 9 / 16)
          .background(SpidolaPalette.set)
        Text(item.title)
          .font(SpidolaType.caption)
          .foregroundStyle(SpidolaPalette.broadcastWhite)
          .lineLimit(1)
        if let subtitle = item.subtitle {
          Text(subtitle)
            .font(SpidolaType.caption)
            .foregroundStyle(SpidolaPalette.staticGray)
            .lineLimit(1)
        }
      }
      .frame(width: cardWidth, alignment: .leading)
    }
    .buttonStyle(.plain)
    .spidolaFocusRing(isFocused: isFocused)
    .accessibilityLabel(
      item.subtitle.map { String(localized: "\(item.title), \($0)", bundle: .module) }
        ?? item.title)
  }
}
