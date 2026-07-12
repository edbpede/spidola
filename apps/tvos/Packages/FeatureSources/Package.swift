// swift-tools-version: 6.3
// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later
import PackageDescription

// FeatureSources — The sources vertical slice: add/manage sources and the pairing screen.
let package = Package(
  name: "FeatureSources",
  platforms: [.tvOS(.v18)],
  products: [
    .library(name: "FeatureSources", targets: ["FeatureSources"])
  ],
  targets: [
    .target(name: "FeatureSources")
  ],
  swiftLanguageModes: [.v6]
)
