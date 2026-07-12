// swift-tools-version: 6.3
// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later
import PackageDescription

// PlayerContract — The engine protocol, the EngineError taxonomy, and the selection policy (TECH_SPEC §8).
let package = Package(
  name: "PlayerContract",
  platforms: [.tvOS(.v18)],
  products: [
    .library(name: "PlayerContract", targets: ["PlayerContract"])
  ],
  targets: [
    .target(name: "PlayerContract")
  ],
  swiftLanguageModes: [.v6]
)
