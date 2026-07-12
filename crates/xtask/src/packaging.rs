// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Native packaging: the tvOS XCFramework and the Android per-ABI libraries (TECH_SPEC §9).
//!
//! These build the `core-api` static/dynamic library for the platform targets and assemble the
//! artifacts the shells link. They require the platform toolchains — the Apple SDKs + Xcode for
//! the XCFramework, and cargo-ndk + the Android NDK for the ABIs — so they run on the CI Apple
//! and Android runners, not the Linux core lane. Each preflights its toolchain and fails with an
//! actionable message rather than a cryptic build error when a piece is missing.
//!
//! The Apple tvOS targets (`aarch64-apple-tvos` and the simulator variants) are Rust **Tier 2**:
//! prebuilt std ships with the pinned stable toolchain, so no `-Z build-std` is needed. The
//! documented fallback (nightly + `build-std`) lives in `docs/toolchains.md` for the case where
//! the pin ever sits behind the promotion.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, bail};

use crate::paths::{cargo, target_dir, workspace_root};

/// The tvOS device target and the two simulator targets (Apple-silicon + Intel).
const TVOS_DEVICE: &str = "aarch64-apple-tvos";
const TVOS_SIM_ARM: &str = "aarch64-apple-tvos-sim";
const TVOS_SIM_X86: &str = "x86_64-apple-tvos";

/// The Android ABIs shipped: two device ABIs plus `x86_64` for the emulator (TECH_SPEC §7).
const ANDROID_ABIS: &[&str] = &["arm64-v8a", "armeabi-v7a", "x86_64"];

/// The static library name Cargo emits for the `staticlib` crate-type.
const STATICLIB: &str = "libcore_api.a";

/// Builds the tvOS XCFramework (device + fat simulator slice) with the generated UniFFI header.
///
/// # Errors
/// Returns an actionable error if a required Rust target, `xcodebuild`, or the generated FFI
/// header is missing, or if any build/assembly step fails.
pub(crate) fn xcframework() -> anyhow::Result<()> {
    let root = workspace_root();
    require_tool("xcodebuild", "Xcode command-line tools")?;
    for target in [TVOS_DEVICE, TVOS_SIM_ARM, TVOS_SIM_X86] {
        require_rust_target(target)?;
        build_static(&root, target)?;
    }

    let out = root.join("target/xcframework");
    let _ = fs::remove_dir_all(&out);
    fs::create_dir_all(&out).with_context(|| format!("create {}", out.display()))?;

    // Fuse the two simulator arches into one fat static library via lipo.
    let sim_fat = out.join("libcore_api-sim.a");
    lipo(
        &[
            static_path(&root, TVOS_SIM_ARM),
            static_path(&root, TVOS_SIM_X86),
        ],
        &sim_fat,
    )?;

    // The XCFramework needs a headers dir carrying the FFI header + a `module.modulemap`.
    let headers = out.join("Headers");
    fs::create_dir_all(&headers)?;
    let generated = root.join("apps/tvos/Packages/CoreKit/Generated");
    copy_into(
        &generated.join("core_apiFFI.h"),
        &headers.join("core_apiFFI.h"),
    )?;
    copy_into(
        &generated.join("core_apiFFI.modulemap"),
        &headers.join("module.modulemap"),
    )
    .context("generated FFI modulemap missing — run `cargo xtask gen-bindings` first")?;

    let xcframework = out.join("CoreFFI.xcframework");
    let status = Command::new("xcodebuild")
        .arg("-create-xcframework")
        .arg("-library")
        .arg(static_path(&root, TVOS_DEVICE))
        .arg("-headers")
        .arg(&headers)
        .arg("-library")
        .arg(&sim_fat)
        .arg("-headers")
        .arg(&headers)
        .arg("-output")
        .arg(&xcframework)
        .status()
        .context("spawn xcodebuild")?;
    if !status.success() {
        bail!("xcodebuild -create-xcframework failed");
    }
    println!("built {}", xcframework.display());
    Ok(())
}

/// Builds the `core-api` shared library for every Android ABI into a `jniLibs` tree via
/// cargo-ndk (consumed by the Gradle AAR/prefab build).
///
/// # Errors
/// Returns an actionable error if cargo-ndk or the NDK is missing, or a build step fails.
pub(crate) fn android() -> anyhow::Result<()> {
    let root = workspace_root();
    require_tool("cargo-ndk", "cargo-ndk (`cargo install cargo-ndk`)")?;
    if std::env::var_os("ANDROID_NDK_HOME").is_none()
        && std::env::var_os("ANDROID_NDK_ROOT").is_none()
    {
        bail!("set ANDROID_NDK_HOME (or ANDROID_NDK_ROOT) to the installed NDK path");
    }

    let jni_libs = root.join("target/jniLibs");
    let _ = fs::remove_dir_all(&jni_libs);
    fs::create_dir_all(&jni_libs)?;

    let mut command = Command::new(cargo());
    command.current_dir(&root).arg("ndk");
    for abi in ANDROID_ABIS {
        command.arg("-t").arg(abi);
    }
    let status = command
        .arg("-o")
        .arg(&jni_libs)
        .args(["build", "-p", "core-api", "--release"])
        .status()
        .context("spawn cargo ndk")?;
    if !status.success() {
        bail!("cargo ndk build failed");
    }
    println!(
        "built Android jniLibs ({}) → {}",
        ANDROID_ABIS.join(", "),
        jni_libs.display()
    );
    Ok(())
}

/// Builds the `core-api` static library for one Apple target.
fn build_static(root: &Path, target: &str) -> anyhow::Result<()> {
    let status = Command::new(cargo())
        .current_dir(root)
        .args(["build", "-p", "core-api", "--release", "--target", target])
        .status()
        .context("spawn cargo build")?;
    if !status.success() {
        bail!("cargo build for {target} failed");
    }
    Ok(())
}

/// Path to the built static library for a target.
fn static_path(root: &Path, target: &str) -> PathBuf {
    target_dir(root)
        .join(target)
        .join("release")
        .join(STATICLIB)
}

/// Fuses several single-arch static libraries into one fat library via `lipo`.
fn lipo(inputs: &[PathBuf], output: &Path) -> anyhow::Result<()> {
    let mut command = Command::new("lipo");
    command.arg("-create");
    for input in inputs {
        command.arg(input);
    }
    let status = command
        .arg("-output")
        .arg(output)
        .status()
        .context("spawn lipo")?;
    if !status.success() {
        bail!("lipo -create failed");
    }
    Ok(())
}

fn copy_into(from: &Path, to: &Path) -> anyhow::Result<()> {
    fs::copy(from, to).with_context(|| format!("copy {} → {}", from.display(), to.display()))?;
    Ok(())
}

/// Fails with an actionable message if `tool` is not on `PATH`.
fn require_tool(tool: &str, hint: &str) -> anyhow::Result<()> {
    let found = Command::new(tool)
        .arg("--version")
        .output()
        .is_ok_and(|out| out.status.success());
    if found {
        Ok(())
    } else {
        bail!("`{tool}` not found on PATH — install {hint}")
    }
}

/// Fails with an actionable message if the Rust `target` is not installed.
fn require_rust_target(target: &str) -> anyhow::Result<()> {
    let output = Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
        .context("spawn rustup")?;
    if String::from_utf8_lossy(&output.stdout)
        .lines()
        .any(|line| line.trim() == target)
    {
        Ok(())
    } else {
        bail!(
            "Rust target `{target}` not installed — run `rustup target add {target}` (Tier 2 stable), \
             or use the nightly `-Z build-std` fallback in docs/toolchains.md"
        )
    }
}
