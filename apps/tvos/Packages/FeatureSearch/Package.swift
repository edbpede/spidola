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
  targets: [
    .target(name: "FeatureSearch")
  ],
  swiftLanguageModes: [.v6]
)
