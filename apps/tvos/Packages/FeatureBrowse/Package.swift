// swift-tools-version: 6.3
// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later
import PackageDescription

// FeatureBrowse — The browse vertical slice: source/type/category/channel drill-down.
let package = Package(
  name: "FeatureBrowse",
  platforms: [.tvOS(.v18)],
  products: [
    .library(name: "FeatureBrowse", targets: ["FeatureBrowse"])
  ],
  dependencies: [
    .package(path: "../CoreKit"),
    .package(path: "../DesignSystem"),
  ],
  targets: [
    .target(
      name: "FeatureBrowse",
      dependencies: ["CoreKit", "DesignSystem"]
    )
  ],
  swiftLanguageModes: [.v6]
)
