// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import CoreKit
import DesignSystem
import FeatureBrowse
import FeaturePlayback
import FeatureSearch
import FeatureSettings
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
  case pairing
  case settings
  /// A picker for one closed-set setting. The payload is the field itself, so the settings slice
  /// gets one picker screen instead of nine, and the app stays a route table rather than learning
  /// what any individual setting means.
  case settingsOptions(SettingsField)
  case diagnostics
}

/// The state-driven navigation shell: a `NavigationStack` whose path is plain state and whose
/// destinations are resolved from `Route`. The app target is a composition root only — it holds the
/// one `SpidolaCore` and hands each feature the narrow access protocol it needs plus a
/// `BrowseNavigator` that pushes routes (TECH_SPEC §3.1: composition only at the shell).
struct RootView: View {
  let core: SpidolaCore
  let registry: EngineRegistry

  @State private var path: [Route] = []

  /// What a phone sent, waiting to pre-fill the add-source form.
  ///
  /// Held here rather than in the `Route` on purpose: an Xtream submission carries a password, and
  /// `Route` is a `Hashable` value designed to be cheap to compare, copy, and — for anyone who
  /// later reaches for `NavigationPath`'s codable restoration — write to disk. A credential has no
  /// business in a type shaped for that. This is plain in-memory state, cleared the moment the form
  /// that consumes it goes away (TECH_SPEC §12).
  @State private var pairedSubmission: PairingSubmission?

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
      manageSources: { path.append(.manageSources) },
      openSettings: { path.append(.settings) })
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
      SourcesView(
        access: core,
        onAddSource: { path.append(.addSource) },
        onPair: { path.append(.pairing) })
    case .addSource:
      AddSourceView(access: core, prefill: pairedSubmission, onFinished: popToManageSources)
    case .pairing:
      PairingView(access: core, onSubmission: openPrefilledAddSource, onCancel: popPairing)
    case .settings:
      SettingsView(access: core, navigator: settingsNavigator)
    case .settingsOptions(let field):
      // Popping is the app's job, not the slice's: the slice knows the write landed, the stack
      // knows what to do about it.
      SettingsOptionsView(field: field, access: core, onFinished: popSettingsOptions)
    case .diagnostics:
      DiagnosticsView(access: core, navigator: settingsNavigator)
    }
  }

  private var settingsNavigator: SettingsNavigator {
    SettingsNavigator(
      openOptions: { field in path.append(.settingsOptions(field)) },
      openDiagnostics: { path.append(.diagnostics) })
  }

  /// Returns from a picker to whichever screen opened it — the settings root or diagnostics, both
  /// of which re-read their snapshot when they reappear.
  private func popSettingsOptions() {
    if let last = path.last, case .settingsOptions = last { path.removeLast() }
  }

  /// Replaces the pairing screen with a pre-filled add-source form, for the person at the TV to
  /// confirm (PRD §6.1). It never adds the source itself: anything on the LAN could have posted
  /// this, and the confirmation is what makes that safe.
  private func openPrefilledAddSource(_ submission: PairingSubmission) {
    pairedSubmission = submission
    if path.last == .pairing { path.removeLast() }
    path.append(.addSource)
  }

  /// Returns from the add-source screen to the sources list, which reloads on reappear.
  private func popToManageSources() {
    if path.last == .addSource { path.removeLast() }
    // A submission carries an Xtream password. It is spent the moment the form is built, so drop
    // it as soon as that form is gone rather than leaving a credential in the navigation shell for
    // the rest of the session (TECH_SPEC §12).
    pairedSubmission = nil
  }

  /// Leaves the pairing screen. Its `.task` is cancelled on the way out, which stops the server.
  private func popPairing() {
    if path.last == .pairing { path.removeLast() }
  }

  /// Leaves playback for the screen it was opened from.
  private func popPlayback() {
    if let last = path.last, case .playback = last { path.removeLast() }
  }
}
