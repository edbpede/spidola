// swift-tools-version: 6.3
// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later
import PackageDescription

// FeatureBrowse — The browse vertical slice: source/type/category/channel drill-down.
let package = Package(
  name: "FeatureBrowse",
  // English-first, with the string infrastructure in place from day one (PRD §6.10). Declaring the
  // default localization is what makes `String(localized:bundle: .module)` resolve against the
  // catalog rather than silently echoing its key back.
  defaultLocalization: "en",
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
      dependencies: ["CoreKit", "DesignSystem"],
      resources: [.process("Resources")]
    ),
    .testTarget(
      name: "FeatureBrowseTests",
      dependencies: ["FeatureBrowse", "CoreKit"]
    ),
  ],
  swiftLanguageModes: [.v6]
)
