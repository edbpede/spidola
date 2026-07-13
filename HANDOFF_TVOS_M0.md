<!--
SPDX-FileCopyrightText: 2026 Spidola contributors
SPDX-License-Identifier: AGPL-3.0-or-later
-->

# Handoff — tvOS M0 walking skeleton

This handoff records the verified state of the tvOS lane on branch
`feat/phase-3-walking-skeleton-m0` as of 2026-07-13. The lane is no longer “written but never
compiled”: the Rust core is wired into Swift, every package and the app build for tvOS, the
simulator test scheme passes, and the fixture/focus flow has been observed.

The only M0 validation intentionally left open is a run on real Apple TV hardware. GitHub-hosted
CI, commit, and push also remain for the next operator.

## Verified environment

- Repo: `/Users/dkp/Documents/GitHub/edbpede/spidola`
- Xcode: 26.6, build `17F113`
- Swift: 6.3.3
- Rust: 1.96.1
- tvOS Simulator runtime: 26.5
- Deployment target: tvOS 18.0
- Local simulator used: Apple TV 4K (3rd generation)
- Supporting tools: XcodeGen 2.45.4, xcbeautify 3.2.1, SwiftLint 0.65.0

Check the pins before doing anything else:

```sh
cd /Users/dkp/Documents/GitHub/edbpede/spidola
tools/ci/assert-toolchains.sh
xcrun simctl list runtimes | rg tvOS
xcrun simctl list devices available | rg 'Apple TV'
```

If an Xcode operation requires privilege on the current machine, use `sudo -n`; never request or
handle the user's password.

## What is now wired

`apps/tvos/Packages/CoreKit/Package.swift` contains:

- a local `CoreFFI` binary target backed by `CoreFFI.xcframework`;
- a `core_api` target that compiles `Generated/core_api.swift` and depends on `CoreFFI`;
- a `CoreKit` target dependency on `core_api`.

The generated header and module map are excluded from the Swift source target because the
XCFramework already publishes the `core_apiFFI` Clang module. Do not hand-edit anything under
`apps/tvos/Packages/CoreKit/Generated/`.

The staged framework and generated Xcode project are build products and are ignored:

```text
apps/tvos/Packages/CoreKit/CoreFFI.xcframework/
apps/tvos/Spidola.xcodeproj/
```

The Rust packager sets `TVOS_DEPLOYMENT_TARGET=18.0` for both device and simulator archives. This
prevents a framework rebuilt with the tvOS 26.5 SDK from acquiring a 26.5 minimum OS version.

## Rebuild and stage the XCFramework

```sh
cd /Users/dkp/Documents/GitHub/edbpede/spidola

cargo run -p xtask -- check-bindings
rustup target add aarch64-apple-tvos aarch64-apple-tvos-sim
cargo run -p xtask -- package-xcframework

rm -rf apps/tvos/Packages/CoreKit/CoreFFI.xcframework
cp -R target/xcframework/CoreFFI.xcframework \
  apps/tvos/Packages/CoreKit/CoreFFI.xcframework
```

The remove-before-copy is intentional: plain `cp -R` is not idempotent when the destination
already exists.

Expected slices:

- `tvos-arm64` for device;
- `tvos-arm64-simulator` for Apple silicon Simulator;
- Clang module `core_apiFFI` in both slices.

## Build the Swift packages correctly

Plain `swift build` targets the macOS host and is not a valid tvOS verification command for these
packages. Build with the tvOS Simulator SDK and triple:

```sh
cd /Users/dkp/Documents/GitHub/edbpede/spidola
sdk="$(xcrun --sdk appletvsimulator --show-sdk-path)"

for pkg in apps/tvos/Packages/*/; do
  echo "== building $pkg =="
  swift build \
    --package-path "$pkg" \
    --triple arm64-apple-tvos18.0-simulator \
    --sdk "$sdk"
done

# Also compile the BrowseModel test bundle.
swift build --build-tests \
  --package-path apps/tvos/Packages/FeatureBrowse \
  --triple arm64-apple-tvos18.0-simulator \
  --sdk "$sdk"
```

The BrowseModel tests cover loading, empty, ready mapping, and retryable error state. They use
XCTest and are also compiled into the app's tvOS UI-test runner, which is the reliable way to
execute both logic and UI tests on this Xcode/runtime combination.

Do not use standalone `xcodebuild test -scheme FeatureBrowse` as the primary local/CI test path.
On this machine Xcode built the raw package test bundle, then intermittently stalled in its
simulator install/launch worker. The app scheme's runner installs and completes consistently.

## Generate and build the app

```sh
cd /Users/dkp/Documents/GitHub/edbpede/spidola/apps/tvos
xcodegen generate

cd /Users/dkp/Documents/GitHub/edbpede/spidola
set -o pipefail
xcodebuild build \
  -project apps/tvos/Spidola.xcodeproj \
  -scheme Spidola \
  -destination 'generic/platform=tvOS Simulator' \
  -derivedDataPath target/DerivedData-tvOS \
  CODE_SIGNING_ALLOWED=NO | xcbeautify
```

`project.yml` excludes `x86_64` for the Simulator because the XCFramework intentionally contains
the Apple-silicon simulator slice only. The app uses the external `App/Info.plist`, bundle ID
`dev.spidola.tv`, version `0.0.0` (build 1), and a tvOS 18.0 minimum deployment target.

The deterministic app product is:

```text
target/DerivedData-tvOS/Build/Products/Debug-appletvsimulator/Spidola.app
```

## Run the complete simulator test gate

Boot a simulator, then run the single scheme. Use `OS=latest` in CI; a UDID is convenient locally.

```sh
cd /Users/dkp/Documents/GitHub/edbpede/spidola

xcrun simctl boot 'Apple TV 4K (3rd generation)' 2>/dev/null || true
xcrun simctl bootstatus 'Apple TV 4K (3rd generation)' -b

set -o pipefail
xcodebuild test \
  -project apps/tvos/Spidola.xcodeproj \
  -scheme Spidola \
  -destination 'platform=tvOS Simulator,name=Apple TV 4K (3rd generation),OS=latest' \
  -derivedDataPath target/DerivedData-tvOS \
  CODE_SIGNING_ALLOWED=NO | xcbeautify
```

The `SpidolaUITests` runner executes five tests:

- four `BrowseModelTests` for loading/empty/ready/error state;
- `SpidolaUITests.testFixtureCatalogAndDpadFocus`, which cold-launches the app, waits for Channel 1,
  verifies initial focus, presses remote Down, and verifies Channel 2 receives focus.

Latest local result: **5 tests, 0 failures**. The UI test keeps a screenshot attachment showing
Channel 2 with the Test-Card Amber focus treatment.

## Runtime behavior verified

The app now:

1. creates and validates the Rust core handshake before rendering browse;
2. seeds a 24-channel fixture through the real core boundary on first launch;
3. removes a failed/empty fixture source so a later launch can recover;
4. waits for seeding before constructing `RootView`, avoiding a first-launch empty-state race;
5. renders the fixture list and moves focus predictably with the tvOS remote.

The following simulator logs were observed in order:

```text
[dev.spidola.tv:spidola::db] core initialized ...
[dev.spidola.tv:spidola::boot] core 0.0.0, schema 1, boundary 1
[dev.spidola.tv:spidola::import] import committed inserted=24 ...
[dev.spidola.tv:spidola::boot] seeded 24 channels
```

Recheck them with:

```sh
xcrun simctl spawn booted log show \
  --last 5m --style compact --info --debug \
  --predicate 'subsystem == "dev.spidola.tv"'
```

This is the M0 core/shell log-interleave evidence: database and import records originate in the
core sink, while handshake and seed records originate in the app shell under the same subsystem.

Computer Use was verified after granting its macOS permissions. It returned the live Simulator
window state and screenshots, launched Spidola from the tvOS Home Screen, showed Channel 1 with
initial focus, and moved the Test-Card Amber focus treatment to Channel 2 after a Down key press.
This direct GUI observation corroborates the XCUITest assertion and retained result-bundle image.

## Full local quality gate

```sh
cd /Users/dkp/Documents/GitHub/edbpede/spidola

tools/ci/assert-toolchains.sh

find apps/tvos -name '*.swift' \
  -not -path '*/.build/*' \
  -not -path '*/Generated/*' \
  -not -path '*/contract-harness/*' \
  -print0 | xargs -0 swift format lint --strict

swiftlint lint --strict
cargo fmt --all -- --check
cargo clippy -p xtask --all-targets -- -D warnings
cargo run -p xtask -- check-bindings
tools/ci/build-contract-harness-tvos.sh
```

Latest local contract result:

```text
HARNESS OK — handshake=0.0.0/schema1/boundary1, import=2000 progress>=5,
cancel=Cancelled, logSink+secrets wired
```

## Apple CI lane

`.github/workflows/apple.yml` now:

- installs the two Rust tvOS targets;
- builds and stages the XCFramework before any Swift package build;
- uses the tvOS Simulator triple/SDK for package compilation;
- generates and builds the Xcode project with `pipefail` enabled;
- executes the five-test app scheme on Apple TV 4K (3rd generation), `OS=latest`;
- excludes `.build`, generated bindings, and the contract harness from hand-written Swift format
  linting.

The workflow has been validated command-for-command locally but has not yet run on GitHub Actions
for these uncommitted changes.

## Remaining optional hardware validation

Real Apple TV hardware was not available and remains unchecked. To finish it:

1. Set a valid `DEVELOPMENT_TEAM` without committing a personal team ID.
2. Regenerate the project with XcodeGen.
3. Pair the Apple TV in Xcode's Devices and Simulators window.
4. Build/run from Xcode and record: cold start, fixture list, D-pad focus, and back navigation.
5. Confirm the same core/shell records in Console.

## Definition of done

- [x] Xcode 26.6, tvOS SDK, and a tvOS Simulator runtime installed and selected.
- [x] `CoreFFI.xcframework` built at tvOS 18.0 and wired into `CoreKit`.
- [x] All `apps/tvos/Packages/*` build for the tvOS Simulator SDK/triple.
- [x] `xcodegen generate` and `xcodebuild build` succeed.
- [x] App cold-launches, imports 24 fixture channels, and renders browse.
- [x] Four BrowseModel state tests pass.
- [x] Simulator D-pad focus smoke test passes with the Test-Card Amber treatment.
- [x] Core and shell logs interleave under `dev.spidola.tv`.
- [x] Format, SwiftLint, Rust xtask checks, binding drift check, and Swift contract harness pass.
- [x] Phase 3 tvOS/simulator plan checkboxes updated only for verified work.
- [ ] Optional real Apple TV run and manual checklist.
- [ ] Run the updated Apple job on GitHub Actions.
- [ ] Commit with a Conventional Commit + DCO sign-off and push/update the PR.

## Reference

- Toolchain pins: `docs/toolchains.md`
- Architecture: `docs/TECH_SPEC.md` (§4.8 logging, §5 FFI, §6 tvOS)
- Apple lane: `.github/workflows/apple.yml`
- Xcode project source: `apps/tvos/project.yml`
- Rust packaging: `crates/xtask/src/packaging.rs`
- Swift socket/FFI reference: `apps/tvos/contract-harness/main.swift`
