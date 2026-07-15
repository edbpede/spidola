// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import CoreKit
import DesignSystem
import SwiftUI

/// The signature (PRD §8.5): a broadcast lower-third that slides up over live video, showing the
/// playing channel with its neighbours peeking above and below for zap-ahead browsing, underlined
/// by a three-point ribbon of the SMPTE bar spectrum.
///
/// The peek is the whole point — it is what makes the strip a *zapping* instrument rather than a
/// caption. The viewer sees where up and down go before pressing, which is how a broadcast tuner
/// has always behaved.
///
/// It renders from state it is handed and starts no work: it must appear in one frame and never
/// stall video (PRD §8.5).
struct ChannelStrip: View {
  let window: ZapWindow?
  let channel: PlayableChannel
  let isLive: Bool

  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  var body: some View {
    VStack(alignment: .leading, spacing: 0) {
      peek(window?.previous, edge: .top)
      band
      SmpteRibbon()
      peek(window?.next, edge: .bottom)
    }
    .background(SpidolaPalette.set.opacity(Self.bandOpacity))
    // A lower-third sits on the lower third. The video above it stays uncovered, which is the
    // difference between a strip and a scrim.
    .frame(maxWidth: .infinity, alignment: .leading)
    .accessibilityElement(children: .combine)
    .accessibilityLabel(accessibilityLabel)
    .accessibilityValue(accessibilityValue)
  }

  /// The band: logo, name, and the live marker. Now/next EPG joins it in Phase 8 — the row is laid
  /// out to take it without moving anything that is already here.
  private var band: some View {
    HStack(spacing: SpidolaSpacing.m) {
      LogoImage(url: channel.logo)
        .frame(width: Self.logoWidth, height: Self.logoWidth * 9 / 16)
        .background(SpidolaPalette.studio)
      VStack(alignment: .leading, spacing: SpidolaSpacing.xs) {
        Text(channel.name)
          .font(SpidolaType.title)
          .foregroundStyle(SpidolaPalette.broadcastWhite)
          .lineLimit(1)
        if let group = channel.group {
          Text(group)
            .font(SpidolaType.caption)
            .foregroundStyle(SpidolaPalette.staticGray)
            .lineLimit(1)
        }
      }
      Spacer(minLength: 0)
      if isLive { LiveMarker() }
      if let position { positionLabel(position) }
    }
    .padding(.horizontal, SpidolaSpacing.xl)
    .padding(.vertical, SpidolaSpacing.m)
  }

  /// An adjacent channel, dimmed and half-height: legible enough to aim at, quiet enough that the
  /// playing channel stays the subject.
  @ViewBuilder private func peek(_ neighbour: PlayableChannel?, edge: VerticalEdge) -> some View {
    if let neighbour {
      HStack(spacing: SpidolaSpacing.s) {
        Image(systemName: edge == .top ? "chevron.up" : "chevron.down")
          .font(SpidolaType.caption)
          .foregroundStyle(SpidolaPalette.staticGray)
        Text(neighbour.name)
          .font(SpidolaType.caption)
          .foregroundStyle(SpidolaPalette.staticGray)
          .lineLimit(1)
        Spacer(minLength: 0)
      }
      .padding(.horizontal, SpidolaSpacing.xl)
      .padding(.vertical, SpidolaSpacing.xs)
      .background(SpidolaPalette.studio.opacity(Self.peekOpacity))
      .accessibilityHidden(true)
    }
  }

  /// Position in the ring, shown only when the ring's length is known — a search ring is paged
  /// without a count, and "3 of ?" is worse than nothing.
  ///
  /// Widened to `Int` before interpolating so the extracted key is a plain `%lld / %lld`, and said
  /// through the catalog rather than by raw interpolation so that the separator — which a listener
  /// hears spoken — is a translator's to place rather than frozen into Swift. Android's own strip
  /// says it as a `%1$d / %2$d` resource, so the two screens count the same by construction.
  private var position: String? {
    guard let window, let total = window.total else { return nil }
    return String(localized: "\(Int(window.offset) + 1) / \(Int(total))", bundle: .module)
  }

  private func positionLabel(_ text: String) -> some View {
    Text(text)
      .font(SpidolaType.caption)
      .foregroundStyle(SpidolaPalette.staticGray)
  }

  /// The strip is named by the channel it is tuned to; that the channel is live and where it sits
  /// in the ring are the tuner's state, not part of its name. Said as one phrase, "BBC One, News,
  /// Live, 3 / 12" gives a listener nothing to tell the channel's own words from the strip's
  /// reading of them — the split is what makes the name announce as a name (PRD §6.10).
  private var accessibilityLabel: String {
    var parts = [channel.name]
    if let group = channel.group { parts.append(group) }
    return parts.joined(separator: ", ")
  }

  private var accessibilityValue: String {
    var parts: [String] = []
    if isLive { parts.append(String(localized: "Live", bundle: .module)) }
    if let position { parts.append(position) }
    return parts.joined(separator: ", ")
  }

  private static let logoWidth: CGFloat = 120
  /// The band is translucent so the video reads through it — a lower-third, not a panel.
  private static let bandOpacity: Double = 0.92
  private static let peekOpacity: Double = 0.75
}

/// The live indicator — one of exactly three things Test-Card Amber is allowed to mark (PRD §8.2).
struct LiveMarker: View {
  var body: some View {
    HStack(spacing: SpidolaSpacing.xs) {
      Circle()
        .fill(SpidolaPalette.testCardAmber)
        .frame(width: Self.dot, height: Self.dot)
      Text(String(localized: "LIVE", bundle: .module))
        .font(SpidolaType.caption)
        .tracking(Self.tracking)
        .foregroundStyle(SpidolaPalette.testCardAmber)
    }
    .accessibilityHidden(true)
  }

  private static let dot: CGFloat = 8
  private static let tracking: CGFloat = 1.5
}
