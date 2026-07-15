// swift-tools-version: 6.3
// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later
import PackageDescription

// FeatureSettings — The settings vertical slice: the full settings surface and diagnostics screen.
let package = Package(
  name: "FeatureSettings",
  // English-first, with the string infrastructure in place from day one (PRD §6.10). Declaring the
  // default localization is what makes `String(localized:bundle: .module)` resolve against the
  // catalog rather than silently echoing its key back.
  defaultLocalization: "en",
  platforms: [.tvOS(.v18)],
  products: [
    .library(name: "FeatureSettings", targets: ["FeatureSettings"])
  ],
  dependencies: [
    .package(path: "../CoreKit"),
    .package(path: "../DesignSystem"),
    // The default-player rows name engines, and engine identities are `PlayerContract`'s to spell
    // (TECH_SPEC §8). Depending on the contract keeps this slice from inventing a second set of
    // keys that would drift from the ones playback actually resolves — and costs no decoder
    // dependency, since the contract itself links no engine.
    .package(path: "../PlayerContract"),
  ],
  targets: [
    .target(
      name: "FeatureSettings",
      dependencies: ["CoreKit", "DesignSystem", "PlayerContract"],
      resources: [.process("Resources")]
    ),
    .testTarget(
      name: "FeatureSettingsTests",
      dependencies: ["FeatureSettings", "CoreKit", "PlayerContract"]
    ),
  ],
  swiftLanguageModes: [.v6]
)
