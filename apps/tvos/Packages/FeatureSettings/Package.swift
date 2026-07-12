// swift-tools-version: 6.3
// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later
import PackageDescription

// FeatureSettings — The settings vertical slice: the full settings surface and diagnostics screen.
let package = Package(
  name: "FeatureSettings",
  platforms: [.tvOS(.v18)],
  products: [
    .library(name: "FeatureSettings", targets: ["FeatureSettings"])
  ],
  targets: [
    .target(name: "FeatureSettings")
  ],
  swiftLanguageModes: [.v6]
)
