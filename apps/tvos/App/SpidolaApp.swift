// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import SwiftUI

/// The tvOS composition root (TECH_SPEC §6). It builds the app container once — wiring the core,
/// Keychain secrets, and OSLog sink — then renders the navigation shell and seeds the M0 fixture
/// catalog through the core.
@main
struct SpidolaApp: App {
  @State private var container = AppContainer()

  var body: some Scene {
    WindowGroup {
      RootView(catalog: container.catalog)
        .task { await container.seedFixtureIfNeeded() }
    }
  }
}
