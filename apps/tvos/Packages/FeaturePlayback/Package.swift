// swift-tools-version: 6.3
// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later
import PackageDescription

// FeaturePlayback — The playback vertical slice: playback UI, the zap path, and channel-strip behaviour.
let package = Package(
  name: "FeaturePlayback",
  platforms: [.tvOS(.v18)],
  products: [
    .library(name: "FeaturePlayback", targets: ["FeaturePlayback"])
  ],
  targets: [
    .target(name: "FeaturePlayback")
  ],
  swiftLanguageModes: [.v6]
)
