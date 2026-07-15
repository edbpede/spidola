// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import CoreKit
import DesignSystem
import SwiftUI
import core_api

/// The categories screen for one source: an optional media-kind selector (shown only when a source
/// carries more than one kind — Xtream, Phase 6) followed by the virtualized list of groups. A
/// group leads to its channel list.
public struct SourceBrowseView: View {
  @State private var model: SourceBrowseModel
  private let sourceId: Int64
  private let sourceName: String
  private let navigator: BrowseNavigator

  @FocusState private var focused: FocusTarget?

  public init(
    sourceId: Int64, sourceName: String, access: any BrowseAccess, navigator: BrowseNavigator
  ) {
    _model = State(initialValue: SourceBrowseModel(sourceId: sourceId, access: access))
    self.sourceId = sourceId
    self.sourceName = sourceName
    self.navigator = navigator
  }

  public var body: some View {
    content
      .frame(maxWidth: .infinity, maxHeight: .infinity)
      .background(SpidolaPalette.studio)
      .navigationTitle(sourceName)
      .task { await model.load() }
  }

  @ViewBuilder private var content: some View {
    switch model.state {
    case .loading:
      ProgressView(String(localized: "Loading categories…", bundle: .module))
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    case .empty:
      CenteredNotice(
        text: String(
          localized: "This source has no channels yet. Refresh it from the sources screen.",
          bundle: .module))
    case .failed(let error):
      actionableError(
        error,
        retry: { Task { await model.load() } },
        goBack: {})
    case .ready(let content):
      ready(content)
    }
  }

  private func ready(_ content: SourceBrowseContent) -> some View {
    ScrollView {
      VStack(alignment: .leading, spacing: SpidolaSpacing.m) {
        if content.kinds.count > 1 {
          kindSelector(content)
        }
        LazyVStack(spacing: SpidolaSpacing.s) {
          ForEach(content.groups, id: \.self) { group in
            let title = group.title ?? String(localized: "Ungrouped", bundle: .module)
            SpidolaRow(
              title: title,
              accessory: .text("\(group.channelCount)"),
              isFocused: focused == .group(title)
            ) {
              navigator.openChannels(sourceId, content.kind, group.title, title)
            }
            .focused($focused, equals: .group(title))
            // The trailing number is a count, and a bare "42" arriving after a group's name could
            // be anything. Said with its unit it means what the column of digits means to someone
            // who can see them line up. Widened to `Int` and pluralised through the catalog for
            // the reasons the import's own count gives (AddSourceView).
            .accessibilityLabel(title)
            .accessibilityValue(
              String(localized: "\(Int(group.channelCount)) channels", bundle: .module)
            )
            .accessibilityIdentifier("group-\(title)")
          }
        }
        .padding(.horizontal, SpidolaSpacing.safeHorizontal)
      }
      .padding(.vertical, SpidolaSpacing.safeVertical)
    }
  }

  private func kindSelector(_ content: SourceBrowseContent) -> some View {
    HStack(spacing: SpidolaSpacing.m) {
      ForEach(content.kinds, id: \.self) { kind in
        let isSelected = kind == content.kind
        Button(kind.label) { Task { await model.select(kind: kind) } }
          .buttonStyle(.plain)
          .padding(.horizontal, SpidolaSpacing.l)
          .padding(.vertical, SpidolaSpacing.s)
          .background(isSelected ? SpidolaPalette.testCardAmber : SpidolaPalette.set)
          .foregroundStyle(isSelected ? SpidolaPalette.studio : SpidolaPalette.broadcastWhite)
          .font(SpidolaType.caption)
          .focused($focused, equals: .kind(kind))
          .spidolaFocusRing(isFocused: focused == .kind(kind))
          // Which kind is showing is carried by the amber fill and nothing else, and a colour does
          // not survive being read aloud.
          .accessibilityValue(
            isSelected
              ? String(localized: "Selected", bundle: .module)
              : String(localized: "Not selected", bundle: .module)
          )
      }
    }
    .padding(.horizontal, SpidolaSpacing.safeHorizontal)
  }

  private enum FocusTarget: Hashable {
    case kind(MediaKind)
    case group(String)
  }
}

/// A centered informational message on the Studio canvas, for empty/placeholder states.
struct CenteredNotice: View {
  let text: String

  var body: some View {
    Text(text)
      .font(SpidolaType.title)
      .foregroundStyle(SpidolaPalette.broadcastWhite)
      .multilineTextAlignment(.center)
      .frame(maxWidth: 900)
      .frame(maxWidth: .infinity, maxHeight: .infinity)
      .padding(SpidolaSpacing.xl)
  }
}
