// swift-tools-version: 6.3
// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later
import PackageDescription

// PlayerMPV — The MPVKit engine implementation (default on tvOS).
let package = Package(
  name: "PlayerMPV",
  platforms: [.tvOS(.v18)],
  products: [
    .library(name: "PlayerMPV", targets: ["PlayerMPV"])
  ],
  targets: [
    .target(name: "PlayerMPV")
  ],
  swiftLanguageModes: [.v6]
)
