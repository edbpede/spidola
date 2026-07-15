// swift-tools-version: 6.3
// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later
import PackageDescription

// FeatureSources — The sources vertical slice: add/manage sources and the pairing screen.
let package = Package(
  name: "FeatureSources",
  // English-first, with the string infrastructure in place from day one (PRD §6.10). Declaring the
  // default localization is what makes `String(localized:bundle: .module)` resolve against the
  // catalog rather than silently echoing its key back.
  defaultLocalization: "en",
  platforms: [.tvOS(.v18)],
  products: [
    .library(name: "FeatureSources", targets: ["FeatureSources"])
  ],
  dependencies: [
    .package(path: "../CoreKit"),
    .package(path: "../DesignSystem"),
  ],
  targets: [
    .target(
      name: "FeatureSources",
      dependencies: ["CoreKit", "DesignSystem"],
      resources: [.process("Resources")]
    ),
    .testTarget(
      name: "FeatureSourcesTests",
      dependencies: ["FeatureSources", "CoreKit"]
    ),
  ],
  swiftLanguageModes: [.v6]
)
