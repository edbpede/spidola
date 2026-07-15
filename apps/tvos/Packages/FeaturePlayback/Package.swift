// swift-tools-version: 6.3
// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later
import PackageDescription

// FeaturePlayback — The playback vertical slice: playback UI, the zap path, and channel-strip behaviour.
//
// It depends on PlayerContract but on no engine: engines are peers injected by the composition
// root (doctrine §3.1), so this slice holds engine identities and never links a decoder.
let package = Package(
  name: "FeaturePlayback",
  // English-first, with the string infrastructure in place from day one (PRD §6.10). Declaring the
  // default localization is what makes `String(localized:bundle: .module)` resolve against the
  // catalog rather than silently echoing its key back.
  defaultLocalization: "en",
  platforms: [.tvOS(.v18)],
  products: [
    .library(name: "FeaturePlayback", targets: ["FeaturePlayback"])
  ],
  dependencies: [
    .package(path: "../CoreKit"),
    .package(path: "../DesignSystem"),
    .package(path: "../PlayerContract"),
  ],
  targets: [
    .target(
      name: "FeaturePlayback",
      dependencies: ["CoreKit", "DesignSystem", "PlayerContract"],
      resources: [.process("Resources")]
    ),
    .testTarget(
      name: "FeaturePlaybackTests",
      dependencies: ["FeaturePlayback", "CoreKit", "PlayerContract"]
    ),
  ],
  swiftLanguageModes: [.v6]
)
