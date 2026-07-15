// swift-tools-version: 6.3
// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later
import PackageDescription

// DesignSystem — Design tokens, focus styles (Test-Card Amber), and the channel-strip components (PRD §8).
let package = Package(
  name: "DesignSystem",
  // English-first, with the string infrastructure in place from day one (PRD §6.10). Declaring the
  // default localization is what makes `String(localized:bundle: .module)` resolve against the
  // catalog rather than silently echoing its key back.
  //
  // This layer displays almost no words of its own — every title, subtitle, and button label is
  // passed in by the slice that owns it. What lives in the catalog here is the *punctuation* that
  // joins two of those values into one screen-reader sentence, which is the caller's to supply and
  // this layer's to arrange.
  defaultLocalization: "en",
  platforms: [.tvOS(.v18)],
  products: [
    .library(name: "DesignSystem", targets: ["DesignSystem"])
  ],
  targets: [
    .target(name: "DesignSystem", resources: [.process("Resources")])
  ],
  swiftLanguageModes: [.v6]
)
