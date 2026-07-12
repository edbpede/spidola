// swift-tools-version: 6.3
// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later
import PackageDescription

// DesignSystem — Design tokens, focus styles (Test-Card Amber), and the channel-strip components (PRD §8).
let package = Package(
  name: "DesignSystem",
  platforms: [.tvOS(.v18)],
  products: [
    .library(name: "DesignSystem", targets: ["DesignSystem"])
  ],
  targets: [
    .target(name: "DesignSystem")
  ],
  swiftLanguageModes: [.v6]
)
