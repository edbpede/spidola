<!--
SPDX-FileCopyrightText: 2026 Spidola contributors
SPDX-License-Identifier: AGPL-3.0-or-later
-->

# Toolchains

Pinned toolchain versions for reproducible builds (TECH_SPEC §9). Every pin here is
mirrored by a machine-readable source of truth in the repository, and CI asserts them:

| Layer | Pin source of truth | CI assertion |
|---|---|---|
| Rust | `rust-toolchain.toml` | rustup honours the file automatically |
| Apple | this file + `apps/tvos/project.yml` | `tools/ci/assert-toolchains.sh` (Xcode/Swift) |
| Android | `apps/androidtv/gradle/libs.versions.toml` + `gradle/wrapper/gradle-wrapper.properties` | root `build.gradle.kts` asserts JDK + Kotlin |

Bumping a pin is a normal PR: change the source-of-truth file, update the row here, and
let the three CI lanes prove the tree still builds green.

## Rust core

| Item | Pin |
|---|---|
| Rust toolchain | **1.96.1** (`rust-toolchain.toml`, `profile = "minimal"`, components `rustfmt` + `clippy`) |
| Edition | 2024 (resolver 3) |
| MSRV | 1.96.1 (`workspace.package.rust-version`) |

### Apple targets and the Tier-2 note

The core builds for `aarch64-apple-tvos` plus its Apple-silicon simulator variant
(`aarch64-apple-tvos-sim`) and is packaged with the generated Swift bindings into an
XCFramework by `xtask` (Phase 2). These two targets are **Tier 2** upstream, so the pinned
stable toolchain ships prebuilt standard libraries for them — no `build-std` needed. The
Intel simulator target (`x86_64-apple-tvos`) remains Tier 3 (no prebuilt std) and is
intentionally not built — CI and current Macs are Apple-silicon-only.

**Fallback (documented, not the default):** should a future pin ever sit behind that
promotion, build the tvOS targets on a nightly toolchain with
`-Z build-std=std,panic_abort` and the appropriate `--target`. This path is retained only
as an escape hatch; the stable Tier-2 route is the supported one.

### FFI bindings and packaging (`xtask`)

The UniFFI boundary is generated and packaged by `cargo xtask` (the cargo-xtask pattern, not
shell scripts):

| Task | What it does |
|---|---|
| `cargo xtask gen-bindings` | (Re)generate the committed Swift + Kotlin bindings in library mode |
| `cargo xtask check-bindings` | Reproducibility gate: fail if committed bindings drift from the Rust definitions (core CI lane) |
| `cargo xtask package-xcframework` | Build the tvOS device + simulator static libs and assemble `CoreFFI.xcframework` (Apple CI lane) |
| `cargo xtask package-android` | Build the per-ABI `libcore_api.so` via cargo-ndk into a `jniLibs` tree (Android CI lane) |

The XCFramework build needs the tvOS Rust targets (`rustup target add aarch64-apple-tvos
aarch64-apple-tvos-sim`); the Android build needs the Android Rust targets (`rustup target add
aarch64-linux-android armv7-linux-androideabi x86_64-linux-android`), `cargo-ndk`, and the
pinned NDK (`ANDROID_NDK_HOME`).

## Apple (tvOS shell)

| Item | Pin |
|---|---|
| Xcode | **26.6.x** (exact build recorded by the Apple CI lane and `assert-toolchains.sh`; ships Swift 6.3.3) |
| Swift | **6.3.3** (Swift 6 language mode, strict concurrency, default-MainActor isolation) |
| SPM tools-version | 6.3 (every local `Package.swift`) |
| tvOS deployment target | **18.0** |
| Project generation | XcodeGen (`apps/tvos/project.yml` → `Spidola.xcodeproj`), so the project is text-reviewable rather than a committed `pbxproj` |
| Default / alternate player | MPVKit (LGPL FFmpeg xcframeworks, `tools/build-mpvkit/`) / AVPlayer |

## Android (Android TV shell)

All versions live in `apps/androidtv/gradle/libs.versions.toml`; the values below are the
Phase-0 pins and are refreshed there.

| Item | Pin |
|---|---|
| JDK (Gradle toolchain) | **21** (Temurin/OpenJDK LTS) |
| Kotlin | **2.4.0** (K2-only compiler; `org.jetbrains.kotlin.plugin.compose`) |
| KSP | **2.3.10** (KSP2 unified versioning, Kotlin 2.4.0 support; never KAPT) |
| Android Gradle Plugin | **8.13.0** |
| Gradle | **8.14** (`gradle-wrapper.properties`) |
| compileSdk / targetSdk | **36** |
| minSdk | **26** |
| NDK | **28.2.13676358** (per-ABI core + libmpv builds, `tools/build-libmpv-android/`) |
| Compose for TV | `androidx.tv:tv-material` **1.1.x** on foundation lazy layouts |
| Navigation | Navigation 3 (`androidx.navigation3`) |
| Default / fallback player | Media3 ExoPlayer **1.10.x** (`media3-ui-compose`) / libmpv (JNI) |
| DI | Hilt (KSP2 processing) |

> Android device/emulator ABIs: `arm64-v8a`, `armeabi-v7a` (devices) and `x86_64` (emulator).

## Local prerequisites

- **Rust:** none beyond `rustup` — the toolchain file installs `1.96.1` on first `cargo` run.
- **Apple:** Xcode `26.6.x`; `swift format` ships with the toolchain; `swiftlint` via
  Homebrew; `xcodegen` via Homebrew.
- **Android:** JDK `21`; the Android SDK (`compileSdk 36`, build-tools, NDK per the table)
  via the SDK manager, `ANDROID_HOME` exported. Gradle itself comes from the committed
  wrapper (`./gradlew`).
