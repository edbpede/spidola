# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

Spidola is an IPTV player for Apple TV and Android TV: one Rust core (`crates/`) exposed over a
single UniFFI boundary to two native shells (`apps/tvos/`, `apps/androidtv/`).

## Architecture Overview

**Rust core — strictly layered, one façade.** `crates/core-model` (domain types, no sibling
dependencies) is the base; `core-parse`, `core-fetch`, `core-db`, `core-search`, `core-xtream`, and
`core-pair` are capabilities that depend downward only. `crates/core-api` is the *only* crate the
shells see: it composes the others, owns the Tokio runtime and the SQLite handle, carries the
`uniffi::setup_scaffolding!()` definitions, and flattens every failure into the `ApiError` taxonomy
(`crates/core-api/src/error.rs`). Nothing in the shells may reach past it.

**The FFI boundary is generated and committed.** `cargo xtask gen-bindings` compiles `core-api` as
a cdylib and emits Swift into `apps/tvos/Packages/CoreKit/Generated/` and Kotlin into
`apps/androidtv/core/corekit/generated/`. Both trees are committed, compiled, and byte-for-byte
reproducible — `cargo xtask check-bindings` fails CI on any drift.

**Both shells mirror the same vertical slices.** tvOS uses local SPM packages
(`apps/tvos/Packages/*`), Android uses Gradle modules (`apps/androidtv/settings.gradle.kts`), and
the names correspond one-to-one: `CoreKit`/`:core:corekit`, `DesignSystem`/`:core:designsystem`,
`PlayerContract`/`:core:player-contract`, the four player engines, and five features (browse,
playback, search, settings, sources). Features never import each other; wiring happens only in the
composition roots (`apps/tvos/App/`, `apps/androidtv/app/src/main/kotlin/dev/spidola/tv/AppContainer.kt`).

## Essential Commands

Three lanes mirror the three CI workflows in `.github/workflows/`. Run `prek install` once, then
`prek run --all-files` for the fast local mirror of the format/lint gates.

### Core (repository root)

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo test -p core-api --test contract                              # one integration-test file
cargo test -p core-parse --test property golden_fixtures_parse_as_expected   # one case
cargo xtask check-bindings                        # bindings drift gate
tools/ci/check-secret-logging.sh                  # no log macro may format a secret
cargo deny check                                  # advisories + license allow-list
python3 tools/community/validate-translations.py
```

### Android TV (from `apps/androidtv/`, after `cargo xtask package-android`)

```sh
./gradlew lintDebug ktlintCheck detekt
./gradlew testDebugUnitTest
./gradlew :core:corekit:testDebugUnitTest --tests 'dev.spidola.tv.core.corekit.ActionableErrorTest'
./gradlew assembleDebug :app:assembleDebugAndroidTest
```

`package-android` writes the per-ABI `libcore_api.so` into `target/jniLibs`, which
`core/corekit/build.gradle.kts` registers as a `jniLibs` source directory — builds that need the
native core fail without it.

### tvOS (repository root; Xcode 26.6.x)

```sh
rustup target add aarch64-apple-tvos aarch64-apple-tvos-sim
cargo xtask package-xcframework
rm -rf apps/tvos/Packages/CoreKit/CoreFFI.xcframework
cp -R target/xcframework/CoreFFI.xcframework apps/tvos/Packages/CoreKit/CoreFFI.xcframework
cd apps/tvos && xcodegen generate
xcodebuild test -project apps/tvos/Spidola.xcodeproj -scheme Spidola \
  -destination 'platform=tvOS Simulator,name=Apple TV 4K (3rd generation),OS=latest'
swiftlint lint --strict                                      # run from the repository root
```

Append `-only-testing:SpidolaUITests/ActionableErrorTests` to `xcodebuild test` for one suite.

## Common Change Workflows

### Changing the exported core surface

1. Edit the UniFFI-exported items in `crates/core-api/` (services under `src/services/`, DTOs in
   `src/records.rs`, errors in `src/error.rs`).
2. Run `cargo xtask gen-bindings` and commit the regenerated Swift + Kotlin trees.
3. Bump `BOUNDARY_VERSION` in `crates/core-api/src/lib.rs` when the surface changes shape; the
   startup handshake uses it so an older shell refuses a newer core legibly instead of crashing.
4. Update the hand-written adapters (`Packages/CoreKit/Sources/CoreKit/`,
   `core/corekit/src/main/kotlin/dev/spidola/tv/core/corekit/`) and both shells' call sites.
5. Keep the three parity harnesses in step: `crates/core-api/tests/contract.rs`,
   `apps/tvos/contract-harness/main.swift`, `apps/androidtv/contract-harness/ContractHarness.kt`.
6. Verify with `cargo test --workspace` and `cargo xtask check-bindings`.

### Changing the database schema

1. Add the *next* numbered migration constant in `crates/core-db/src/migrations.rs`. Never edit a
   shipped migration — they are forward-only, and a downgraded app that meets a newer
   `user_version` refuses rather than guessing.
2. Bump `SCHEMA_VERSION` in the same file; the boundary handshake reports it.
3. Tables are `STRICT`. The FTS5 index is contentless with `contentless_delete=1` and maintained by
   triggers — extend the triggers rather than writing to the index directly.
4. Secrets never enter SQLite: persist `username` plus an opaque `secret_ref` and keep the value
   behind the host `SecretStore` callback.

### Touching playback

Engine work goes through the shared contract (`PlayerContract` / `:core:player-contract`:
`PlaybackEngine`, `EngineError`, `EngineRegistry`, `EngineSelection`); both shells keep a
`FakeEngine` for host tests. Read `docs/engine-acceptance.md` first, and drive real decoders with
the repository headend: `tools/test-headend/headend.sh generate|start|stop`.

## Repository Conventions

- **Every commit is DCO-signed** (`git commit -s`) and follows Conventional Commits; both are
  hook-enforced, and a `no-commit-to-branch` hook blocks `main`. Branch and open a PR.
- **Every new file carries an inline SPDX header** (`AGPL-3.0-or-later`). Markdown prose is instead
  covered by the `*.md` aggregate annotation in `REUSE.toml`.
- **Workspace lints are opt-in**: a new crate needs `[lints] workspace = true`, plus
  `#![forbid(unsafe_code)]` unless it hosts FFI glue (only `core-api` does).
- **Shared dependency versions live only in the root `Cargo.toml`**; members reference them with
  `{ workspace = true }`. `anyhow` is application-only (`xtask`, tests) — libraries define
  `thiserror` types.
- **No junk-drawer modules.** `utils`, `helpers`, `misc`, and `manager` are banned by the
  modularity doctrine; shared behaviour earns a named home or stays where it is used.
- Complexity/length lints (`too_many_lines`, `cognitive_complexity`) are advisory review prompts.
  Under CI's `-D warnings`, answer a firing lint with a refactor or a scoped, commented
  `#[allow(...)]` recording the single concept — never by raising a threshold in `clippy.toml`.

Two standing rules are review blockers (`CONTRIBUTING.md`):

- No bare `unwrap`/`expect` on a fallible Rust path, no untyped or swallowed Swift error, no
  caught-and-ignored Kotlin exception. Map every new failure into the layer's error taxonomy.
- Every new subsystem lands with tracing spans (core) or subsystem/category logging (shells), with
  secrets provably absent; `tools/ci/check-secret-logging.sh` enforces the second half.

## Implementation Decisions

| Situation | Use | Avoid |
|---|---|---|
| A shell needs core behaviour | A `core-api` service method | Depending on `core-db`/`core-fetch`/etc. from a shell |
| New developer automation | A `cargo xtask` subcommand in `crates/xtask/src/` | A new shell script under `tools/` |
| Generated bindings are wrong | Regenerate via `cargo xtask gen-bindings`; put adapters in `CoreKit`/`corekit` sources | Hand-editing `Generated/` or `generated/` |
| Android TV UI component | `androidx.tv.material3` with `androidx.compose.foundation` lazy layouts | `androidx.compose.material3`, `TvLazyRow`, Leanback |
| New Swift package test | A suite under `Packages/*/Tests/`, registered in the `SpidolaUITests` target | `swift test` (cannot run a tvOS-triple package) |

## Critical Gotchas

- **`swift test` cannot run these packages** — they are tvOS-triple only. Every package test suite
  is compiled into the `SpidolaUITests` target in `apps/tvos/project.yml` and runs under
  `xcodebuild test`; a new `Tests/` directory must be added to that target's `sources`.
- **`apps/tvos/Spidola.xcodeproj` and `CoreFFI.xcframework` are gitignored build products.**
  Regenerate them with `xcodegen generate` and `cargo xtask package-xcframework`, and edit
  `apps/tvos/project.yml` rather than a `pbxproj`.
- **The generated binding trees sit outside the source sets deliberately** and are excluded from the
  whitespace fixers, ktlint, detekt, swift-format, and SwiftLint (`prek.toml`). Reformatting them
  breaks the drift check.
- **rustls only, never OpenSSL.** `reqwest` carries both native and webpki roots because
  `aarch64-apple-tvos` exposes no system trust store and would otherwise load zero roots.
- **Toolchain pins are asserted in CI.** Bumping one means editing its source of truth
  (`rust-toolchain.toml`, `apps/androidtv/gradle/libs.versions.toml`, `apps/tvos/project.yml`) *and*
  the matching row in `docs/toolchains.md`.
- **Translations are dual-catalog.** The six Xcode String Catalogs and six Android `strings.xml`
  files listed in `crowdin.yml` must stay in lockstep; run
  `python3 tools/community/validate-translations.py` after touching either side.

## Additional Documentation

- `docs/TECH_SPEC.md` — the normative architecture reference. Read before any cross-layer change
  (§3.1 modularity doctrine, §4.7 errors, §4.8 logging, §5 FFI, §8 engine contract, §12 security).
- `docs/IMPLEMENTATION_PLAN.md` — phase scope and exit criteria. Read to learn what is in scope now
  (Phase 7 is software-complete; Phase 8 is deferred hardware work).
- `docs/PRD.md` — product requirements and performance budgets. Read when changing user-visible
  behaviour.
- `docs/toolchains.md` — every version pin plus the `xtask` task table. Read before bumping a
  toolchain or adding a build target.
- `docs/engine-acceptance.md` — the four-engine acceptance suite and test-headend usage. Read before
  touching playback.
- `.augment/rules/rust-dev-pro.md`, `swift-dev-pro.md`, `kotlin-dev-pro.md` — **normative** language
  standards, anti-pattern tables included. Consult the one for the layer you are editing.
- `CONTRIBUTING.md` — licensing terms, DCO, and the two standing rules. Read before a first commit.
- `tools/licenses/README.md` — read when adding or updating a shell dependency; both gates fail closed.
- `tools/release/README.md` — read when producing a direct Android artifact or a changelog preview.
