// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import SwiftUI

/// The tvOS composition root (TECH_SPEC §6). It builds the app container once — wiring the core,
/// Keychain secrets, and OSLog sink — then renders the navigation shell and seeds the M0 fixture
/// catalog through the core.
@main
struct SpidolaApp: App {
  @Environment(\.scenePhase) private var scenePhase
  @State private var container = AppContainer()
  @State private var isReady = false
  @State private var pendingDeepLink: URL?

  var body: some Scene {
    WindowGroup {
      root
        .onChange(of: scenePhase) { _, phase in
          guard phase == .background else { return }
          Task { await TopShelfSnapshotWriter.refresh(from: container.core) }
        }
    }
  }

  @ViewBuilder private var root: some View {
    #if DEBUG
      if let configuration = EngineAcceptanceConfiguration.current {
        EngineAcceptanceView(configuration: configuration)
      } else {
        normalRoot
      }
    #else
      normalRoot
    #endif
  }

  private var normalRoot: some View {
    Group {
      if isReady {
        RootView(
          core: container.core,
          registry: container.registry,
          pendingDeepLink: $pendingDeepLink)
      } else {
        ProgressView("Preparing fixture catalog…")
      }
    }
    .onOpenURL { pendingDeepLink = $0 }
    .onContinueUserActivity("dev.spidola.browse") { activity in
      guard
        let deepLink = activity.userInfo?["deepLink"] as? String,
        let url = URL(string: deepLink)
      else { return }
      pendingDeepLink = url
    }
    .task {
      await container.seedFixtureIfNeeded()
      await TopShelfSnapshotWriter.refresh(from: container.core)
      isReady = true
    }
  }
}
