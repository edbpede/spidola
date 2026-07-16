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

  var body: some Scene {
    WindowGroup {
      Group {
        if isReady {
          RootView(core: container.core, registry: container.registry)
        } else {
          ProgressView("Preparing fixture catalog…")
        }
      }
      .task {
        await container.seedFixtureIfNeeded()
        await TopShelfSnapshotWriter.refresh(from: container.core)
        isReady = true
      }
      .onChange(of: scenePhase) { _, phase in
        guard phase == .background else { return }
        Task { await TopShelfSnapshotWriter.refresh(from: container.core) }
      }
    }
  }
}
