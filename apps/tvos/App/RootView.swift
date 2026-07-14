// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import CoreKit
import DesignSystem
import FeatureBrowse
import FeaturePlayback
import FeatureSearch
import FeatureSources
import PlayerContract
import SwiftUI
import core_api

/// The typed navigation route set (TECH_SPEC §6). Every destination beyond the home root is a
/// value in this enum; the state-driven `NavigationStack` resolves each to a feature view. Payloads
/// are `Hashable` primitives (and the `Hashable` `PlayableChannel`/`MediaKind`), so a route is
/// cheap to push and compare.
enum Route: Hashable {
  case source(id: Int64, name: String)
  case channels(sourceId: Int64, kind: MediaKind, group: String?, title: String)
  /// A channel plus the ring it was chosen from, carried so that pressing Play hands playback the
  /// zap context the viewer's own path implies (PRD §8.4).
  case channel(PlayableChannel, ZapContext, UInt32)
  case playback(PlayableChannel, ZapContext, UInt32)
  case search
  case manageSources
  case addSource
}

/// The state-driven navigation shell: a `NavigationStack` whose path is plain state and whose
/// destinations are resolved from `Route`. The app target is a composition root only — it holds the
/// one `SpidolaCore` and hands each feature the narrow access protocol it needs plus a
/// `BrowseNavigator` that pushes routes (TECH_SPEC §3.1: composition only at the shell).
struct RootView: View {
  let core: SpidolaCore
  let registry: EngineRegistry

  @State private var path: [Route] = []

  var body: some View {
    NavigationStack(path: $path) {
      HomeView(access: core, navigator: navigator)
        .navigationDestination(for: Route.self, destination: destination)
    }
    .spidolaTheme()
  }

  private var navigator: BrowseNavigator {
    BrowseNavigator(
      openSource: { id, name in path.append(.source(id: id, name: name)) },
      openChannels: { sourceId, kind, group, title in
        path.append(.channels(sourceId: sourceId, kind: kind, group: group, title: title))
      },
      openChannel: { channel, context, offset in
        path.append(.channel(channel, context, offset))
      },
      openSearch: { path.append(.search) },
      manageSources: { path.append(.manageSources) })
  }

  @ViewBuilder private func destination(_ route: Route) -> some View {
    switch route {
    case .source(let id, let name):
      SourceBrowseView(sourceId: id, sourceName: name, access: core, navigator: navigator)
    case .channels(let sourceId, let kind, let group, let title):
      ChannelsView(
        sourceId: sourceId, kind: kind, group: group, title: title,
        access: core, navigator: navigator)
    case .channel(let channel, let context, let offset):
      ChannelDetailView(
        channel: channel, access: core,
        onPlay: { path.append(.playback(channel, context, offset)) })
    case .playback(let channel, let context, let offset):
      PlaybackView(
        channel: channel, context: context, offset: offset, access: core, registry: registry,
        onExit: popPlayback)
    case .search:
      SearchView(
        access: core,
        onOpenChannel: { channel, context, offset in
          path.append(.channel(channel, context, offset))
        })
    case .manageSources:
      SourcesView(access: core, onAddSource: { path.append(.addSource) })
    case .addSource:
      AddSourceView(access: core, onFinished: popToManageSources)
    }
  }

  /// Returns from the add-source screen to the sources list, which reloads on reappear.
  private func popToManageSources() {
    if path.last == .addSource { path.removeLast() }
  }

  /// Leaves playback for the screen it was opened from.
  private func popPlayback() {
    if let last = path.last, case .playback = last { path.removeLast() }
  }
}
