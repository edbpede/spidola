<!--
SPDX-FileCopyrightText: 2026 Spidola contributors
SPDX-License-Identifier: AGPL-3.0-or-later
-->

# Handoff — bring the tvOS lane to green (Milestone M0)

Audience: an AI agent with **computer-use control of this Mac** (GUI + terminal). Goal: take the
tvOS shell from "written but never compiled" to **building, linting, testing, and running** on the
tvOS Simulator, then (optionally) on a real Apple TV — matching the Apple CI lane in
`.github/workflows/apple.yml`.

This is the companion to PR #5 (`feat/phase-3-walking-skeleton-m0`). The **Android** lane in that PR
is already implemented and verified. The **tvOS** lane is faithful groundwork: every Swift file
passes `swift format --strict`, but nothing has been compiled because the machine that produced it
had **no Xcode / tvOS SDK**. Expect to make small fixes; this document tells you exactly where.

---

## 0. Current state — read this first

- Repo root: `/Users/dkp/Documents/GitHub/edbpede/spidola`. Work on branch
  `feat/phase-3-walking-skeleton-m0` (already pushed; PR #5 open against `main`).
- The tvOS source you must get compiling (all new, unverified):
  - `apps/tvos/Packages/DesignSystem/Sources/DesignSystem/` — `SpidolaPalette`, `SpidolaType`,
    `SpidolaSpacing`, `SpidolaFocusRing`, `SpidolaTheme`.
  - `apps/tvos/Packages/CoreKit/Sources/CoreKit/` — `SpidolaCore`, `KeychainSecretStore`,
    `OSLogSink`. These `import core_api` (the generated UniFFI Swift module) — **this import does
    not resolve yet** (see Step 3 + Step 4, the critical wiring).
  - `apps/tvos/Packages/FeatureBrowse/Sources/FeatureBrowse/` — `BrowseUiState`, `BrowseModel`,
    `BrowseView`. `FeatureBrowse/Package.swift` was updated to depend on `CoreKit` + `DesignSystem`.
  - `apps/tvos/App/` — `SpidolaApp`, `RootView`, `AppContainer` (composition root + fixture seeder).
- The generated bindings already exist and are committed at
  `apps/tvos/Packages/CoreKit/Generated/` (`core_api.swift`, `core_apiFFI.h`,
  `core_apiFFI.modulemap`). **Do not hand-edit them** — they are build artifacts.
- Pinned toolchains live in `docs/toolchains.md`: **Xcode 26.6.x (Swift 6.3.3)**, tvOS deployment
  target **18.0**, **Rust 1.96.1**. `tools/ci/assert-toolchains.sh` enforces them.

> The single most important task is **Step 4**: wire the compiled Rust core (an XCFramework) and the
> generated Swift bindings into the `CoreKit` Swift package so `import core_api` resolves. Until that
> is done, nothing that touches the core compiles.

---

## 1. Install and select Xcode 26.6 (pinned)

Prefer the `xcodes` CLI (scriptable, exact-version). GUI/App Store also works but is slower.

```sh
# Homebrew must exist; install it first if `which brew` is empty:
#   /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"

brew install xcodesorg/made/xcodes   # the community-standard Xcode installer CLI
brew install aria2                   # faster, more reliable Apple downloads (recommended by xcodes)

xcodes update
xcodes install 26.6                  # downloads + installs to /Applications; will prompt for Apple ID
xcodes select 26.6                   # make it active
sudo xcodebuild -license accept      # accept the license non-interactively
xcode-select -p                      # sanity: should point at .../Xcode-26.6.app/Contents/Developer
xcodebuild -version                  # sanity: Xcode 26.6, Build ...; Swift should report 6.3.3
```

If `xcodes install` fails on Apple ID / 2FA over CLI, fall back to the **Mac App Store** (GUI):
open App Store, search "Xcode", install, then run `sudo xcode-select -s /Applications/Xcode.app`
and `sudo xcodebuild -license accept`.

### Install the tvOS platform + Simulator runtime

Xcode 26 does not bundle every platform by default. Install the tvOS SDK + a Simulator runtime:

```sh
# Preferred (Xcode 16+ / 26): downloads the current tvOS platform for this Xcode.
xcodebuild -downloadPlatform tvOS

# If the above stalls on a 26.x runtime, use xcodes (list, then install the matching tvOS):
xcodes runtimes                      # find the exact "tvOS 26.x" string
xcodes runtimes install "tvOS 26.x"  # substitute the real version from the list

# Verify a tvOS Simulator runtime + a usable device exist:
xcrun simctl list runtimes | grep -i tvos
xcrun simctl list devicetypes | grep -i "Apple TV"
```

Create/boot an "Apple TV" simulator if none is listed:

```sh
xcrun simctl create "Apple TV" "com.apple.CoreSimulator.SimDeviceType.Apple-TV-4K-3rd-generation-1080p" "com.apple.CoreSimulator.SimRuntime.tvOS-26-x"
open -a Simulator
```

---

## 2. Install the supporting CLI tools

```sh
brew install swiftlint xcodegen xcbeautify
swiftlint version        # sanity
xcodegen --version       # sanity
swift format --version   # bundled with the toolchain; should be ~6.3.x
```

`swift format` (SwiftSyntax-based) and `swiftlint` (SourceKit-based) are both required by CI.
`swiftlint` needs a real Xcode selected — that is why it could not run on the previous machine.

---

## 3. Build the Rust core into the tvOS XCFramework

The Swift `CoreKit` links the Rust core through an XCFramework produced by `xtask`. Rust is pinned by
`rust-toolchain.toml`; `rustup` installs 1.96.1 automatically on first `cargo` use.

```sh
cd /Users/dkp/Documents/GitHub/edbpede/spidola

# Confirm the committed bindings still match the Rust definitions (no drift):
cargo run -p xtask -- check-bindings

# tvOS Rust targets are Tier 2 (prebuilt std ships with the pinned stable toolchain — no build-std):
rustup target add aarch64-apple-tvos aarch64-apple-tvos-sim

# Build device + simulator static libs and assemble the framework:
cargo run -p xtask -- package-xcframework
# -> produces target/xcframework/CoreFFI.xcframework
ls -la target/xcframework/CoreFFI.xcframework
```

`CoreFFI.xcframework` bundles `core_apiFFI` (the C FFI, exposed as a Clang module via its
`module.modulemap`). The generated `Generated/core_api.swift` is the Swift layer that `import`s
`core_apiFFI`.

---

## 4. CRITICAL — wire the XCFramework + generated bindings into `CoreKit`

Today `apps/tvos/Packages/CoreKit/Package.swift` compiles only `Sources/CoreKit` and has **no
reference to the XCFramework or the generated Swift**. That was fine when `CoreKit` was an empty
stub; now that `SpidolaCore.swift` does `import core_api`, the package will not build until you add:

1. a **binary target** for `CoreFFI.xcframework` (provides the `core_apiFFI` C module), and
2. a **`core_api`** Swift target that compiles `Generated/core_api.swift` and depends on `CoreFFI`,
3. and make `CoreKit` depend on `core_api`.

### Recommended layout

Copy the framework next to the package (keeps `Package.swift` paths clean; git-ignore the copy):

```sh
cp -R target/xcframework/CoreFFI.xcframework apps/tvos/Packages/CoreKit/CoreFFI.xcframework
printf '\n# Local UniFFI framework (built by `cargo xtask package-xcframework`)\napps/tvos/Packages/CoreKit/CoreFFI.xcframework/\n' >> .gitignore
```

Then set `apps/tvos/Packages/CoreKit/Package.swift` to (adjust names only if the build complains):

```swift
// swift-tools-version: 6.3
// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later
import PackageDescription

let package = Package(
  name: "CoreKit",
  platforms: [.tvOS(.v18)],
  products: [
    .library(name: "CoreKit", targets: ["CoreKit"])
  ],
  targets: [
    .binaryTarget(name: "CoreFFI", path: "CoreFFI.xcframework"),
    .target(
      name: "core_api",
      dependencies: ["CoreFFI"],
      path: "Generated",
      // Compile only the generated Swift; the header + modulemap ship inside the XCFramework.
      sources: ["core_api.swift"]
    ),
    .target(name: "CoreKit", dependencies: ["core_api"]),
  ],
  swiftLanguageModes: [.v6]
)
```

> Alternative without copying: point the binary target at
> `path: "../../../../target/xcframework/CoreFFI.xcframework"`. The copy-and-gitignore approach is
> less fragile and mirrors how the app build consumes it.

### The Apple CI lane needs the framework too

`.github/workflows/apple.yml`'s `apple` job builds each SPM package with `swift build` but never
builds the XCFramework (only the separate `contract` job does). Once `CoreKit` depends on it, add
these steps to the `apple` job **before** "Build SPM packages":

```yaml
      - name: Add tvOS Rust targets
        run: rustup target add aarch64-apple-tvos aarch64-apple-tvos-sim
      - name: Build + stage the UniFFI XCFramework
        run: |
          cargo run -p xtask -- package-xcframework
          cp -R target/xcframework/CoreFFI.xcframework apps/tvos/Packages/CoreKit/CoreFFI.xcframework
```

(Optionally teach `xtask package-xcframework` to also drop the copy into
`apps/tvos/Packages/CoreKit/` so local and CI paths are identical.)

---

## 5. Build the SPM packages and fix compile errors

Build in dependency order; DesignSystem and CoreKit first, then FeatureBrowse.

```sh
cd apps/tvos
for pkg in Packages/DesignSystem Packages/CoreKit Packages/FeatureBrowse; do
  echo "== $pkg =="; ( cd "$pkg" && swift build ) || break
done
```

The code is unverified — expect a handful of fixes. Likely spots, in priority order:

- **`CoreKit/Sources/CoreKit/SpidolaCore.swift`** — the `Source.id` extension pattern-matches
  `case .m3uUrl(let id, _, _, _, _)` etc. Confirm the associated-value arity against the real
  generated `Source` enum in `Generated/core_api.swift` and adjust the underscores if it differs.
  Also confirm `Core.init(config:secrets:logSink:)`, `SourceService.list()/addM3uUrl/refresh`,
  and `CatalogService.channels(sourceId:offset:limit:)` signatures match the generated code.
- **Sendable / actor isolation** — `SpidolaCore`, `KeychainSecretStore`, `OSLogSink`, and the
  private `ImportListenerAdapter` are `final class`es intended to satisfy the generated protocols'
  `Sendable` requirement without `@unchecked`. If the compiler objects, prefer an `actor` or a
  truly-immutable design over `@unchecked Sendable` (project rule: never `@unchecked Sendable`).
- **`FeatureBrowse/.../BrowseView.swift`** — `@FocusState private var focusedID: Int64?` with
  `.focused($focusedID, equals:)`, and `.buttonStyle(.plain)` + `.spidolaFocusRing(isFocused:)`.
  Verify focus visuals actually appear; tune if `.plain` suppresses too much.
- **`App/AppContainer.swift`** — `URL.documentsDirectory` (tvOS 17+), the POSIX loopback server
  (mirrors `apps/tvos/contract-harness/main.swift`; that file is proven), and the
  `for await event in core.importSource(id:)` consumption.
- **`App/SpidolaApp.swift`** — `@State private var container = AppContainer()` where
  `AppContainer` is `@MainActor`. If SwiftUI complains about isolation at the App entry point,
  construct the container lazily inside the scene instead.

Re-run `swift format --in-place --recursive Packages App` after edits, then
`swift format lint --strict` to stay clean.

---

## 6. Generate the Xcode project and run the app on the tvOS Simulator

```sh
cd apps/tvos
xcodegen generate                     # produces Spidola.xcodeproj from project.yml (kept out of git)

# Build for the Simulator (no signing needed):
xcodebuild build \
  -project Spidola.xcodeproj \
  -scheme Spidola \
  -destination 'platform=tvOS Simulator,name=Apple TV' \
  CODE_SIGNING_ALLOWED=NO | xcbeautify

# Install + launch on a booted simulator to actually see it:
xcrun simctl boot "Apple TV" 2>/dev/null || true
open -a Simulator
APP=$(find ~/Library/Developer/Xcode/DerivedData -name 'Spidola.app' -path '*tvOS*' | head -1)
xcrun simctl install booted "$APP"
xcrun simctl launch --console booted dev.spidola.tv
```

**Verify by observation (this is the M0 point):**

- The browse list renders channels from the fixture (the app seeds "Fixture Catalog" over loopback
  on first launch — see `AppContainer.seedFixtureIfNeeded()`). If it stays on the empty state,
  check the boot logs: `xcrun simctl spawn booted log stream --predicate 'subsystem == "dev.spidola.tv"'`
  and confirm `spidola::boot` / `spidola::import` records appear (this also proves the OSLog sink
  and the core→shell log interleave required by the M0 exit criteria).
- Drive the **D-pad** with the Simulator remote (Hardware ▸ or the on-screen remote): focus should
  move predictably and the focused row should show the Test-Card Amber ring + lift.
- If loopback seeding misbehaves in the sandbox, it is acceptable for M0 to instead point the seeder
  at a small on-disk file served locally; the important invariant is that channels arrive **through
  the core**, never fabricated in the shell.

---

## 7. Lint, test, and mirror the full Apple CI lane locally

```sh
cd /Users/dkp/Documents/GitHub/edbpede/spidola

tools/ci/assert-toolchains.sh                       # rustc 1.96.1, Swift 6.3.x, Xcode present

# swift-format (strict) over hand-written tvOS sources (excludes generated + harness):
find apps/tvos -name '*.swift' -not -path '*/Generated/*' -not -path '*/contract-harness/*' \
  -print0 | xargs -0 swift format lint --strict

swiftlint lint --strict                             # now works with Xcode selected

# Swift Testing per package. Add a BrowseModel test that mirrors the Android BrowseViewModelTest
# (fake CatalogAccess -> loading/empty/ready/error). Put it under
# Packages/FeatureBrowse/Tests/FeatureBrowseTests/ and declare a test target in that Package.swift.
for pkg in apps/tvos/Packages/*/; do ( cd "$pkg" && swift test ) || true; done

# Optionally run the whole thing exactly as CI does:
#   act -j apple      # if `act` (nektos/act) is installed, otherwise push and watch GitHub Actions
```

Also confirm the **FFI parity keel** still passes (it links the host core, not the XCFramework):

```sh
tools/ci/build-contract-harness-tvos.sh
```

---

## 8. (Optional) Run on a real Apple TV

Real hardware is part of the M0 exit criteria and cannot be automated end-to-end.

1. In `apps/tvos/project.yml`, set `settings.base.DEVELOPMENT_TEAM` to a valid 10-char Apple
   Developer Team ID (currently empty), then `xcodegen generate` again. Keep `CODE_SIGN_STYLE:
   Automatic`.
2. Pair the Apple TV to Xcode: **Xcode ▸ Window ▸ Devices and Simulators ▸ Apple TV** (the TV must
   be on the same network; enter the pairing code shown on the TV). GUI step — use computer-use.
3. Build/run to the device:
   ```sh
   xcrun devicectl list devices                     # find the Apple TV UDID
   xcodebuild -project apps/tvos/Spidola.xcodeproj -scheme Spidola \
     -destination 'platform=tvOS,id=<UDID>' build
   # then Run from Xcode (GUI) so provisioning + install are handled, or use devicectl to install.
   ```
4. Record a short manual checklist (cold start, browse renders, D-pad focus, back navigation) in the
   PR — that is the "manual checklist recorded" M0 item.

---

## 9. Close out

- Update `docs/IMPLEMENTATION_PLAN.md` Phase 3 **only for what you actually verified**: check the
  three **tvOS shell** sub-items once the app builds + runs, and add the emulator/simulator smoke
  tests + hardware checklist under **CI completion**. The **Exit criteria (= M0)** line is satisfied
  by evidence, not a checkbox — do not toggle it.
- Commit with Conventional Commits **and DCO sign-off** (`git commit -s`); the `prek` hooks enforce
  both. Java must be on `PATH` for the ktlint/detekt hooks:
  `export JAVA_HOME=$(/usr/libexec/java_home -v 21); export PATH="$JAVA_HOME/bin:$PATH"`.
  With Xcode present, do **not** skip `swiftlint` anymore.
- Push to `feat/phase-3-walking-skeleton-m0` to update PR #5, or open a stacked PR.

---

## Known issues / gotchas

- **`import core_api` fails** → Step 4 not done (no XCFramework binary target). This is expected on a
  fresh checkout.
- **`swiftlint` fatal: "Loading sourcekitdInProc.framework failed"** → no real Xcode is selected;
  run `xcode-select -p` and `sudo xcode-select -s /Applications/Xcode-26.6.app/Contents/Developer`.
- **Android context (not your job, for awareness):** Hilt is deferred on Android because
  Dagger 2.57/2.57.1 cannot read Kotlin 2.4 class metadata ("maximum supported version is 2.2.0").
  Manual constructor DI is used instead. If you also touch Android, do not re-enable Hilt until a
  Kotlin-2.4-compatible Dagger exists.
- **Loopback fixture seeding on tvOS:** binding/serving on `127.0.0.1` generally does **not** trigger
  the Local Network privacy prompt (that is for LAN peers), but if the OS blocks it, add
  `NSLocalNetworkUsageDescription` to `App/Info.plist`. The real add-source flow replaces this
  scaffolding in Phase 4.
- **Do not commit** `Spidola.xcodeproj`, `.build/`, `DerivedData/`, or the copied
  `CoreFFI.xcframework` — they are build products (gitignored / to be gitignored).
- **Do not hand-edit** anything under `Generated/` or `.../corekit/generated/` — regenerate with
  `cargo run -p xtask -- gen-bindings` if the core surface changes.

## Definition of done (tvOS M0)

- [ ] Xcode 26.6 + tvOS SDK + a tvOS Simulator runtime installed and selected.
- [ ] `CoreFFI.xcframework` built and wired into `CoreKit` (`import core_api` resolves).
- [ ] All `apps/tvos/Packages/*` build with `swift build`.
- [ ] `xcodegen generate` + `xcodebuild build` (tvOS Simulator) succeed.
- [ ] App launches on the Simulator, browses the fixture catalog, D-pad focus works with the
      Test-Card Amber treatment, and core+shell logs interleave under `dev.spidola.tv`.
- [ ] `swift format lint --strict` and `swiftlint --strict` clean; Swift Testing passes.
- [ ] (Optional) Runs on a real Apple TV; manual checklist recorded.
- [ ] Plan checkboxes updated; changes committed with DCO sign-off and pushed.

## Reference

- Pins: `docs/toolchains.md`. Architecture: `docs/TECH_SPEC.md` (§5 FFI, §6 tvOS, §4.8 logging).
- Apple CI lane: `.github/workflows/apple.yml`. Toolchain assertion: `tools/ci/assert-toolchains.sh`.
- Packaging task: `crates/xtask/src/packaging.rs` (`package-xcframework`). Proven Swift socket
  pattern: `apps/tvos/contract-harness/main.swift`.
- xcodes CLI: <https://github.com/XcodesOrg/xcodes>. Simulator runtimes:
  <https://developer.apple.com/documentation/xcode/installing-additional-simulator-runtimes>.
