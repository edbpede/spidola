// swift-tools-version: 6.3
// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later
import PackageDescription

// PlayerAV — The AVPlayer engine implementation (alternate, HLS-native).
let package = Package(
  name: "PlayerAV",
  platforms: [.tvOS(.v18)],
  products: [
    .library(name: "PlayerAV", targets: ["PlayerAV"])
  ],
  targets: [
    .target(name: "PlayerAV")
  ],
  swiftLanguageModes: [.v6]
)
