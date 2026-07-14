// swift-tools-version: 6.3
// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later
import PackageDescription

// PlayerMPV — The MPVKit engine implementation (default on tvOS).
//
// The MPVKit dependency is pinned with `exact:` rather than a range: the binary xcframeworks it
// vends are checksum-verified against `tools/build-mpvkit/mpvkit.lock`, and a range would let a
// resolve silently move off the checksummed set. Bumping the pin is a deliberate edit in both
// places, verified by `tools/build-mpvkit/verify-mpvkit-pin.sh`.
//
// The product is "MPVKit" — the **LGPL** build. "MPVKit-GPL" exists upstream and must never be
// linked: GPL is incompatible with the App Store posture (PRD §10, TECH_SPEC §12). The verify
// script fails the build if that product name appears anywhere in the tree.
let package = Package(
  name: "PlayerMPV",
  platforms: [.tvOS(.v18)],
  products: [
    .library(name: "PlayerMPV", targets: ["PlayerMPV"])
  ],
  dependencies: [
    .package(path: "../PlayerContract"),
    .package(url: "https://github.com/mpvkit/MPVKit.git", exact: "0.41.0"),
  ],
  targets: [
    .target(
      name: "PlayerMPV",
      dependencies: [
        "PlayerContract",
        .product(name: "MPVKit", package: "MPVKit"),
      ]
    ),
    .testTarget(name: "PlayerMPVTests", dependencies: ["PlayerMPV"]),
  ],
  swiftLanguageModes: [.v6]
)
