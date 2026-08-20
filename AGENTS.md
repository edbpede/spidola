# AGENTS.md

This file provides guidance to AI coding agents when working with code in this
repository.

## Shape

Rust core (`crates/`) compiled to a static/shared library per platform, a UniFFI binding
layer, and two native shells: `apps/tvos` (SwiftUI, local SPM packages) and
`apps/androidtv` (Compose for TV, Gradle). `core-api` is the only crate the shells ever
see. Playback lives in the shells, never in the core.

## Commands

```sh
# Rust core (repo root)
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo test -p core-parse --test property golden_fixtures_parse_as_expected  # single file / case

# Android TV (cd apps/androidtv)
./gradlew testDebugUnitTest
./gradlew :core:corekit:testDebugUnitTest --tests 'dev.spidola.tv.core.corekit.ActionableErrorTest'
./gradlew lintDebug ktlintCheck detekt

# tvOS (repo root; needs `xcodegen generate` in apps/tvos first)
xcodebuild test -project apps/tvos/Spidola.xcodeproj -scheme Spidola \
  -destination 'platform=tvOS Simulator,name=Apple TV 4K (3rd generation),OS=latest'
# single case: add -only-testing:SpidolaUITests/ActionableErrorTests/testEveryBoundaryErrorHasAnAction

# All local gates at once (install once with `prek install`)
prek run --all-files
```

`cargo xtask <task>` is a workspace alias (`.cargo/config.toml`) for the developer
automation binary: `gen-bindings`, `check-bindings`, `package-xcframework`,
`package-android`, `phase1-verify`.

## Build prerequisites the shells will not tell you about

Both shells link native artifacts that are gitignored build products. A fresh clone
cannot build either app until they are produced:

- tvOS: `cargo xtask package-xcframework`, then copy `target/xcframework/CoreFFI.xcframework`
  to `apps/tvos/Packages/CoreKit/CoreFFI.xcframework` (a `binaryTarget` path in
  `Packages/CoreKit/Package.swift`).
- Android: `cargo xtask package-android` writes `target/jniLibs`, wired in as a
  `jniLibs.srcDir` by `apps/androidtv/core/corekit/build.gradle.kts:24`.
- tvOS Rust targets: `rustup target add aarch64-apple-tvos aarch64-apple-tvos-sim`.
  Android: `rustup target add aarch64-linux-android armv7-linux-androideabi
  x86_64-linux-android` plus `cargo-ndk` and the pinned NDK in `ANDROID_NDK_HOME`.

## Generated files — never hand-edit

- UniFFI bindings: `apps/tvos/Packages/CoreKit/Generated/` and
  `apps/androidtv/core/corekit/generated/`. They are committed and must match
  `cargo xtask gen-bindings` byte-for-byte; `cargo xtask check-bindings` is a CI gate.
  Formatters and linters exclude these paths on purpose (`prek.toml`, the Apple/Android
  CI lanes, ktlint/detekt filters in `core/corekit/build.gradle.kts`) — reformatting them
  fails the drift check. Add any new exclusion in all of those places.
- `apps/tvos/Spidola.xcodeproj` is XcodeGen output and gitignored. Change
  `apps/tvos/project.yml` and re-run `xcodegen generate`; never edit the project.

## Invariants

- Changing the FFI surface means updating three contract harnesses in lockstep —
  `crates/core-api/tests/contract.rs`, `apps/tvos/contract-harness/main.swift`,
  `apps/androidtv/contract-harness/ContractHarness.kt` — which drive the same fixture flow.
- `swift test` cannot run the tvOS-triple packages. Package test suites are compiled into
  the `SpidolaUITests` target instead; a new `Packages/*/Tests/*` directory silently never
  runs until it is added to `SpidolaUITests.sources` in `apps/tvos/project.yml`.
- Workspace lints are not inherited. Every new crate needs `[lints] workspace = true`, and
  shared dependency versions go in the root `[workspace.dependencies]`, referenced as
  `{ workspace = true }`.
- `mod.rs` is banned: a module lives in a file of its name with children in a sibling
  directory.
- Complexity and length lints (clippy `cognitive_complexity`/`too_many_lines`, detekt,
  SwiftLint) are advisory by design but CI runs at deny-warnings, so a fired lint blocks.
  Answer it with a refactor or a scoped, commented `#[allow(...)]`/`// swiftlint:disable`,
  not by raising a threshold in `clippy.toml`, `config/detekt/detekt.yml`, or `.swiftlint.yml`.
- Secrets never reach logs: `tools/ci/check-secret-logging.sh` fails any log macro in
  `crates/` that formats `.expose(...)`. Log the redacted `Debug` of the `Secret` type.
- Features never import each other in either shell; anything two features need moves down
  a layer, and wiring happens only in the composition roots (`apps/tvos/App`,
  `apps/androidtv/app`, and `core-api`'s constructor path).
- Every new file carries an SPDX header (`AGPL-3.0-or-later` for project code); `reuse lint`
  is a CI gate. Files that cannot carry one get an entry in `REUSE.toml`, where Markdown is
  already blanket-annotated.
- Benchmarks are a CI regression gate driven by `tools/ci/criterion-benchmarks.txt`, which
  maps package → bench → source path → criterion metric id. Renaming a bench or its
  criterion id without updating that file breaks the gate.
- The media stacks must stay on the LGPL builds — the GPL variants build and run fine but
  cannot ship. `tools/build-mpvkit/verify-mpvkit-pin.sh` and
  `tools/build-libmpv-android/verify-pins.sh` enforce the pins in CI.
- Translation sources are the catalogs listed in `crowdin.yml`; validate edits with
  `python3 tools/community/validate-translations.py` (also a prek hook and CI step).

## Commits

Conventional Commits, DCO sign-off (`git commit -s`), and never commit to `main` — branch
and open a PR. All three are hook-enforced by `prek.toml`; the DCO check on PRs is the
authoritative gate.

## Reference

- `.agents/rules/rust-dev-pro.md` — normative Rust 1.96 / edition 2024 style, including its
  anti-pattern tables. Read before writing Rust.
- `.agents/rules/swift-dev-pro.md` — normative Swift 6.3 / SwiftUI / Observation style.
  Read before writing Swift.
- `.agents/rules/kotlin-dev-pro.md` — normative Kotlin 2.4 / Compose for TV style, notably
  D-pad focus correctness. Read before writing Kotlin or TV UI.
- `CONTRIBUTING.md` — licensing terms, the two standing rules (error handling, logging),
  modularity doctrine summary. Read before your first change to any layer.
- `docs/TECH_SPEC.md` — §3 repo layout and doctrine, §4 core internals, §5 FFI boundary,
  §8 player engine contract, §9 CI, §12 security and license engineering. Read the relevant
  section before designing across a seam.
- `docs/toolchains.md` — pinned toolchain versions and the `xtask` task table. Read before
  bumping a toolchain or debugging a build-environment failure.
- `docs/PRD.md` — product scope, non-goals, platform parity policy. Read before adding a
  user-facing feature.
- `docs/IMPLEMENTATION_PLAN.md` — phase-by-phase plan and acceptance criteria. Read to place
  new work in a phase.
- `docs/phase7-decisions.md` — deferred scope with evidence (EPG grid, recording, phone
  ports). Read before implementing something that looks missing.
- `docs/engine-acceptance.md` + `tools/test-headend/README.md` — real-engine acceptance
  matrix and the local test origin. Read before touching playback.
- `tools/licenses/README.md` — shell dependency license gates and notice generation. Read
  before adding a SwiftPM or Gradle dependency.
- `tools/release/README.md` — changelog and release tooling. Read when cutting a release.
