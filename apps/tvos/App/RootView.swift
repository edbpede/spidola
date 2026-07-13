// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import CoreKit
import DesignSystem
import FeatureBrowse
import SwiftUI

/// The typed navigation route set. The M0 skeleton has a single destination; more land in later
/// phases and slot into the same state-driven stack.
enum Route: Hashable {
  case browse
}

/// The state-driven navigation shell (TECH_SPEC §6): a `NavigationStack` whose path is plain
/// state and whose destinations are resolved from a typed route enum. The app target is a
/// composition root only — it hands the core catalog down and renders the browse slice.
struct RootView: View {
  let catalog: any CatalogAccess

  @State private var path: [Route] = []

  var body: some View {
    NavigationStack(path: $path) {
      BrowseView(catalog: catalog)
        .navigationDestination(for: Route.self) { route in
          switch route {
          case .browse:
            BrowseView(catalog: catalog)
          }
        }
    }
    .spidolaTheme()
  }
}
