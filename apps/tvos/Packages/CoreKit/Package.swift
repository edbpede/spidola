// swift-tools-version: 6.3
// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later
import PackageDescription

// CoreKit — The UniFFI binding wrapper plus Swift adapters: main-actor trampolining for
// callbacks, the Keychain-backed secrets callback, and the OSLog sink (TECH_SPEC §6, §4.8).
let package = Package(
  name: "CoreKit",
  platforms: [.tvOS(.v18)],
  products: [
    .library(name: "CoreKit", targets: ["CoreKit"])
  ],
  targets: [
    .target(name: "CoreKit")
  ],
  swiftLanguageModes: [.v6]
)
