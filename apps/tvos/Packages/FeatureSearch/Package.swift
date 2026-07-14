// swift-tools-version: 6.3
// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later
import PackageDescription

// FeatureSearch — The search vertical slice: global search with per-keystroke results.
let package = Package(
  name: "FeatureSearch",
  platforms: [.tvOS(.v18)],
  products: [
    .library(name: "FeatureSearch", targets: ["FeatureSearch"])
  ],
  dependencies: [
    .package(path: "../CoreKit"),
    .package(path: "../DesignSystem"),
  ],
  targets: [
    .target(
      name: "FeatureSearch",
      dependencies: ["CoreKit", "DesignSystem"]
    ),
    .testTarget(
      name: "FeatureSearchTests",
      dependencies: ["FeatureSearch", "CoreKit"]
    ),
  ],
  swiftLanguageModes: [.v6]
)
