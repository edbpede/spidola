// swift-tools-version: 6.3
// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later
import PackageDescription

// CoreKit — The UniFFI binding wrapper plus Swift adapters: main-actor trampolining for
// callbacks, the Keychain-backed secrets callback, and the OSLog sink (TECH_SPEC §6, §4.8).
let package = Package(
  name: "CoreKit",
  defaultLocalization: "en",
  platforms: [.tvOS(.v18)],
  products: [
    .library(name: "CoreKit", targets: ["CoreKit"])
  ],
  targets: [
    .binaryTarget(name: "CoreFFI", path: "CoreFFI.xcframework"),
    .target(
      name: "core_api",
      dependencies: ["CoreFFI"],
      path: "Generated",
      exclude: ["core_apiFFI.h", "core_apiFFI.modulemap"],
      sources: ["core_api.swift"]
    ),
    .target(
      name: "CoreKit", dependencies: ["core_api"],
      resources: [.process("Resources")]
    ),
    .testTarget(name: "CoreKitTests", dependencies: ["CoreKit"]),
  ],
  swiftLanguageModes: [.v6]
)
