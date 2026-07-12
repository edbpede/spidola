// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! UniFFI binding generation and the CI drift check (TECH_SPEC §5, §9).
//!
//! Bindings are generated in **library mode**: `core-api` is compiled to a cdylib, then the
//! `uniffi-bindgen` helper binary introspects that library's embedded metadata and emits the
//! Swift and Kotlin sources. The generated files are committed so the shells build without a
//! generation step; a reproducibility check (`check-bindings`) regenerates into a scratch
//! directory and fails on any drift between the committed copies and the Rust definitions.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, anyhow, bail};

use crate::paths::{cargo, target_dir, workspace_root};

/// A binding target: the language `uniffi-bindgen` emits, the committed output directory
/// (relative to the workspace root), and the sub-path within it that is purely generated (so
/// the drift check never compares hand-written wrapper code sitting alongside it).
struct Target {
    language: &'static str,
    out_dir: &'static str,
    generated_subdir: &'static str,
}

const TARGETS: &[Target] = &[
    Target {
        language: "swift",
        // Outside `Sources/` so a standalone `swift build` of CoreKit ignores it until Phase 3
        // wires the XCFramework binary target; the app build and the drift check consume it here.
        out_dir: "apps/tvos/Packages/CoreKit/Generated",
        generated_subdir: "",
    },
    Target {
        language: "kotlin",
        // Outside the Gradle source set so `assembleDebug`/ktlint/detekt ignore it until Phase 3
        // wires corekit's UniFFI wrapper; the drift check and the Kotlin harness consume it here.
        out_dir: "apps/androidtv/core/corekit/generated",
        generated_subdir: "uniffi",
    },
];

/// Generates the Swift and Kotlin bindings into their committed locations.
///
/// # Errors
/// Propagates any build or generation failure.
pub(crate) fn generate() -> anyhow::Result<()> {
    let root = workspace_root();
    let library = build_cdylib(&root)?;
    for target in TARGETS {
        let out_dir = root.join(target.out_dir);
        fs::create_dir_all(&out_dir).with_context(|| format!("create {}", out_dir.display()))?;
        run_bindgen(&root, &library, target.language, &out_dir)?;
        println!(
            "generated {} bindings → {}",
            target.language, target.out_dir
        );
    }
    Ok(())
}

/// Regenerates the bindings into a scratch directory and fails if they differ from the
/// committed copies (the CI reproducibility gate).
///
/// # Errors
/// Returns an error listing the drifted files, or propagates a build/generation failure.
pub(crate) fn check() -> anyhow::Result<()> {
    let root = workspace_root();
    let library = build_cdylib(&root)?;
    let scratch = tempfile::tempdir().context("create scratch dir")?;

    let mut drift = Vec::new();
    for target in TARGETS {
        let fresh_root = scratch.path().join(target.language);
        fs::create_dir_all(&fresh_root)?;
        run_bindgen(&root, &library, target.language, &fresh_root)?;

        let committed = root.join(target.out_dir).join(target.generated_subdir);
        let fresh = fresh_root.join(target.generated_subdir);
        drift.extend(diff_dirs(&committed, &fresh, target.language));
    }

    if drift.is_empty() {
        println!("bindings are up to date (no drift)");
        Ok(())
    } else {
        for line in &drift {
            eprintln!("drift: {line}");
        }
        Err(anyhow!(
            "committed bindings drifted from the Rust definitions ({} file(s)); run `cargo xtask gen-bindings`",
            drift.len()
        ))
    }
}

/// Builds the `core-api` cdylib and returns the path to the produced dynamic library.
fn build_cdylib(root: &Path) -> anyhow::Result<PathBuf> {
    let status = Command::new(cargo())
        .current_dir(root)
        .args(["build", "-p", "core-api", "--lib"])
        .status()
        .context("spawn cargo build")?;
    if !status.success() {
        bail!("cargo build -p core-api failed");
    }
    let name = format!(
        "{}core_api{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_SUFFIX
    );
    let library = target_dir(root).join("debug").join(&name);
    if !library.exists() {
        bail!("expected cdylib not found at {}", library.display());
    }
    Ok(library)
}

/// Runs the `uniffi-bindgen` helper binary in library mode for one language.
fn run_bindgen(root: &Path, library: &Path, language: &str, out_dir: &Path) -> anyhow::Result<()> {
    let status = Command::new(cargo())
        .current_dir(root)
        .args([
            "run",
            "--quiet",
            "-p",
            "xtask",
            "--bin",
            "uniffi-bindgen",
            "--",
        ])
        .arg("generate")
        .arg("--library")
        .arg(library)
        .arg("--language")
        .arg(language)
        .arg("--out-dir")
        .arg(out_dir)
        .arg("--no-format")
        .status()
        .context("spawn uniffi-bindgen")?;
    if !status.success() {
        bail!("uniffi-bindgen ({language}) failed");
    }
    Ok(())
}

/// Compares two directory trees byte-for-byte, returning a human-readable description of every
/// added, removed, or changed file.
fn diff_dirs(committed: &Path, fresh: &Path, label: &str) -> Vec<String> {
    let committed_files = snapshot(committed);
    let fresh_files = snapshot(fresh);
    let mut keys: Vec<&PathBuf> = committed_files.keys().chain(fresh_files.keys()).collect();
    keys.sort_unstable();
    keys.dedup();

    let mut drift = Vec::new();
    for key in keys {
        match (committed_files.get(key), fresh_files.get(key)) {
            (Some(a), Some(b)) if a == b => {}
            (Some(_), Some(_)) => drift.push(format!("{label}: {} changed", key.display())),
            (Some(_), None) => drift.push(format!(
                "{label}: {} is stale (no longer generated)",
                key.display()
            )),
            (None, Some(_)) => drift.push(format!(
                "{label}: {} is missing (not committed)",
                key.display()
            )),
            (None, None) => {}
        }
    }
    drift
}

/// Reads every file under `root` into a map keyed by its path relative to `root`.
fn snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut files = BTreeMap::new();
    collect(root, root, &mut files);
    files
}

fn collect(base: &Path, dir: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return; // absent directory → empty snapshot (drift surfaces as "missing")
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(base, &path, out);
        } else if let (Ok(rel), Ok(bytes)) = (path.strip_prefix(base), fs::read(&path)) {
            out.insert(rel.to_path_buf(), bytes);
        }
    }
}
