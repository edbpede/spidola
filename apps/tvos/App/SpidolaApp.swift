// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import SwiftUI

// Phase 0 skeleton: the App target is a composition root only (TECH_SPEC §6). It wires
// the CoreKit adapter and the feature slices in Phase 3 (the walking-skeleton milestone).
@main
struct SpidolaApp: App {
  var body: some Scene {
    WindowGroup {
      RootView()
    }
  }
}
