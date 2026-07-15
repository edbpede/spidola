// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import CoreKit
import DesignSystem
import SwiftUI
import core_api

/// The global search screen: a text field with per-keystroke results, source and media-kind
/// filters, and a focusable result list (PRD §9). Selecting a result opens its detail.
public struct SearchView: View {
  @State private var model: SearchModel
  /// Carries the ring alongside the channel: a result opened from here zaps through the result set
  /// (PRD §8.4). `offset` is the row's position in that set.
  private let onOpenChannel: (PlayableChannel, ZapContext, UInt32) -> Void

  @FocusState private var focused: Focus?

  public init(
    access: any SearchAccess,
    onOpenChannel: @escaping (PlayableChannel, ZapContext, UInt32) -> Void
  ) {
    _model = State(initialValue: SearchModel(access: access))
    self.onOpenChannel = onOpenChannel
  }

  public var body: some View {
    VStack(alignment: .leading, spacing: SpidolaSpacing.m) {
      searchField
      filters
      results
    }
    .padding(.horizontal, SpidolaSpacing.safeHorizontal)
    .padding(.vertical, SpidolaSpacing.safeVertical)
    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    .background(SpidolaPalette.studio)
    .navigationTitle(String(localized: "Search", bundle: .module))
    .task { await model.loadSources() }
    .onAppear { focused = .field }
  }

  private var searchField: some View {
    TextField(String(localized: "Search channels", bundle: .module), text: $model.query)
      .textFieldStyle(.plain)
      .font(SpidolaType.title)
      .foregroundStyle(SpidolaPalette.broadcastWhite)
      .padding(SpidolaSpacing.m)
      .background(SpidolaPalette.set)
      .focused($focused, equals: .field)
      .spidolaFocusRing(isFocused: focused == .field)
      .accessibilityIdentifier("search-field")
      .onChange(of: model.query) { model.scheduleSearch() }
  }

  private var filters: some View {
    ScrollView(.horizontal, showsIndicators: false) {
      HStack(spacing: SpidolaSpacing.s) {
        filterChip(
          String(localized: "All sources", bundle: .module),
          selected: model.sourceFilter == nil, focus: .allSources
        ) {
          model.sourceFilter = nil
          model.scheduleSearch()
        }
        ForEach(model.sources, id: \.id) { source in
          filterChip(
            source.name, selected: model.sourceFilter == source.id, focus: .source(source.id)
          ) {
            model.sourceFilter = source.id
            model.scheduleSearch()
          }
        }
      }
    }
  }

  private func filterChip(
    _ label: String, selected: Bool, focus: Focus, action: @escaping () -> Void
  ) -> some View {
    Button(label, action: action)
      .buttonStyle(.plain)
      .padding(.horizontal, SpidolaSpacing.m)
      .padding(.vertical, SpidolaSpacing.s)
      .background(selected ? SpidolaPalette.testCardAmber : SpidolaPalette.set)
      .foregroundStyle(selected ? SpidolaPalette.studio : SpidolaPalette.broadcastWhite)
      .font(SpidolaType.caption)
      .focused($focused, equals: focus)
      .spidolaFocusRing(isFocused: focused == focus)
      // Which filter is in force is carried by the amber fill and nothing else, and a colour does
      // not survive being read aloud — leaving a listener to guess which of these results they are
      // hearing.
      .accessibilityValue(
        selected
          ? String(localized: "Selected", bundle: .module)
          : String(localized: "Not selected", bundle: .module)
      )
  }

  @ViewBuilder private var results: some View {
    switch model.state {
    case .idle:
      CenteredHint(
        text: String(localized: "Type to search across your channels.", bundle: .module))
    case .loading:
      CenteredHint(text: String(localized: "Searching…", bundle: .module))
    case .empty:
      CenteredHint(text: String(localized: "No channels match “\(model.query)”.", bundle: .module))
    case .failed(let error):
      actionableError(error, retry: { model.scheduleSearch() }, goBack: {})
    case .results(let results):
      resultList(results)
    }
  }

  private func resultList(_ results: SearchResults) -> some View {
    ScrollView {
      LazyVStack(spacing: SpidolaSpacing.s) {
        if results.fuzzy {
          Text(String(localized: "Showing closest matches", bundle: .module))
            .font(SpidolaType.caption)
            .foregroundStyle(SpidolaPalette.staticGray)
        }
        // The set is fetched from offset 0 in score order, so a row's index in it is its offset in
        // the ring.
        ForEach(Array(results.channels.enumerated()), id: \.element.identity) { offset, channel in
          SpidolaRow(
            title: channel.name,
            subtitle: channel.groupTitle,
            isFocused: focused == .result(channel.identity)
          ) {
            onOpenChannel(PlayableChannel(channel), results.context, UInt32(offset))
          }
          .focused($focused, equals: .result(channel.identity))
          .accessibilityIdentifier("search-result-\(channel.name)")
        }
      }
    }
  }

  private enum Focus: Hashable {
    case field
    case allSources
    case source(Int64)
    case result(Int64)
  }
}

/// A centered hint on the Studio canvas, for the idle/searching/empty states.
private struct CenteredHint: View {
  let text: String

  var body: some View {
    Text(text)
      .font(SpidolaType.body)
      .foregroundStyle(SpidolaPalette.staticGray)
      .frame(maxWidth: .infinity, maxHeight: .infinity)
  }
}
