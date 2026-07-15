// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import CoreKit
import DesignSystem
import SwiftUI
import core_api

/// The home screen (PRD §8.3): the favorites row first, then recently watched, then the sources to
/// browse into, with search and source-management reachable from here. The composition root hands
/// it a `HomeAccess` and a `BrowseNavigator`; it holds no durable state of its own.
public struct HomeView: View {
  @State private var model: HomeModel
  private let navigator: BrowseNavigator

  @FocusState private var focused: FocusTarget?

  public init(access: any HomeAccess, navigator: BrowseNavigator) {
    _model = State(initialValue: HomeModel(access: access))
    self.navigator = navigator
  }

  public var body: some View {
    content
      .frame(maxWidth: .infinity, maxHeight: .infinity)
      .background(SpidolaPalette.studio)
      .task { await model.load() }
  }

  @ViewBuilder private var content: some View {
    switch model.state {
    case .loading:
      ProgressView("Loading…")
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    case .empty:
      emptyState
    case .failed(let error):
      actionableError(
        error,
        retry: { Task { await model.load() } },
        goBack: { Task { await model.load() } })
    case .ready(let home):
      ready(home)
    }
  }

  private var emptyState: some View {
    VStack(spacing: SpidolaSpacing.l) {
      Text("Welcome to Spidola")
        .font(SpidolaType.display)
        .foregroundStyle(SpidolaPalette.broadcastWhite)
      Text("Add a playlist or account to start watching.")
        .font(SpidolaType.body)
        .foregroundStyle(SpidolaPalette.staticGray)
      Button("Add a source") { navigator.manageSources() }
        .buttonStyle(.plain)
        .padding(.horizontal, SpidolaSpacing.l)
        .padding(.vertical, SpidolaSpacing.m)
        .background(SpidolaPalette.testCardAmber)
        .foregroundStyle(SpidolaPalette.studio)
        .font(SpidolaType.body)
        .focused($focused, equals: .manage)
        .spidolaFocusRing(isFocused: focused == .manage)
        .accessibilityIdentifier("home-add-source")
    }
    .frame(maxWidth: .infinity, maxHeight: .infinity)
    .onAppear { focused = .manage }
  }

  private func ready(_ home: HomeContent) -> some View {
    ScrollView {
      VStack(alignment: .leading, spacing: SpidolaSpacing.xl) {
        PosterRail(title: "Favorites", items: home.favorites.map(Self.poster)) { item in
          // The row is loaded from offset 0 in ring order, so a poster's index in it is its offset
          // in the favourites ring.
          if let offset = home.favorites.firstIndex(where: { $0.id == item.id }) {
            navigator.openChannel(home.favorites[offset], .favorites, UInt32(offset))
          }
        }
        PosterRail(title: "Recently watched", items: home.recents.map(Self.poster)) { item in
          if let channel = home.recents.first(where: { $0.id == item.id }) {
            // Recents are a history, not a ring: they are ordered by when they were watched, and a
            // core query cannot resolve neighbours for them. Zap is unavailable rather than faked.
            navigator.openChannel(channel, .single, 0)
          }
        }
        sourcesSection(home.sources)
        recentsControls(home)
      }
      .padding(.vertical, SpidolaSpacing.safeVertical)
    }
  }

  private func recentsControls(_ home: HomeContent) -> some View {
    VStack(alignment: .leading, spacing: SpidolaSpacing.s) {
      Text("Recently watched")
        .font(SpidolaType.title)
        .foregroundStyle(SpidolaPalette.broadcastWhite)
        .padding(.horizontal, SpidolaSpacing.safeHorizontal)
      LazyVStack(spacing: SpidolaSpacing.s) {
        SpidolaRow(
          title: "Keep recently watched",
          accessory: .text(home.recentsEnabled ? "On" : "Off"),
          isFocused: focused == .recentsToggle
        ) {
          Task { await model.setRecentsEnabled(!home.recentsEnabled) }
        }
        .focused($focused, equals: .recentsToggle)
        .accessibilityIdentifier("home-recents-toggle")
        if !home.recents.isEmpty {
          SpidolaRow(
            title: "Clear recently watched", accessory: .symbol("trash"),
            isFocused: focused == .recentsClear
          ) {
            Task { await model.clearRecents() }
          }
          .focused($focused, equals: .recentsClear)
          .accessibilityIdentifier("home-recents-clear")
        }
      }
      .padding(.horizontal, SpidolaSpacing.safeHorizontal)
    }
  }

  private func sourcesSection(_ sources: [Source]) -> some View {
    VStack(alignment: .leading, spacing: SpidolaSpacing.s) {
      Text("Sources")
        .font(SpidolaType.title)
        .foregroundStyle(SpidolaPalette.broadcastWhite)
        .padding(.horizontal, SpidolaSpacing.safeHorizontal)
      LazyVStack(spacing: SpidolaSpacing.s) {
        ForEach(sources.filter { $0.common.enabled }, id: \.id) { source in
          SpidolaRow(
            title: source.name,
            subtitle: source.kindLabel,
            isFocused: focused == .source(source.id)
          ) {
            navigator.openSource(source.id, source.name)
          }
          .focused($focused, equals: .source(source.id))
          .accessibilityIdentifier("source-\(source.name)")
        }
        SpidolaRow(
          title: "Search channels", accessory: .symbol("magnifyingglass"),
          isFocused: focused == .search
        ) {
          navigator.openSearch()
        }
        .focused($focused, equals: .search)
        .accessibilityIdentifier("home-search")
        SpidolaRow(
          title: "Add or manage sources", accessory: .symbol("slider.horizontal.3"),
          isFocused: focused == .manage
        ) {
          navigator.manageSources()
        }
        .focused($focused, equals: .manage)
        .accessibilityIdentifier("home-manage")
        SpidolaRow(
          title: "Settings", accessory: .symbol("gearshape"),
          isFocused: focused == .settings
        ) {
          navigator.openSettings()
        }
        .focused($focused, equals: .settings)
        .accessibilityIdentifier("home-settings")
      }
      .padding(.horizontal, SpidolaSpacing.safeHorizontal)
    }
  }

  private static func poster(_ channel: PlayableChannel) -> PosterItem {
    PosterItem(id: channel.id, title: channel.name, subtitle: channel.group, logo: channel.logo)
  }

  private enum FocusTarget: Hashable {
    case source(Int64)
    case search
    case manage
    case settings
    case recentsToggle
    case recentsClear
  }
}
