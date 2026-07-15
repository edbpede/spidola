// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Resolves the core's git revision into `SPIDOLA_GIT_REVISION` for the diagnostics screen
//! (PRD §6.9, `core_api::GIT_REVISION`).
//!
//! A build that cannot see git metadata — a release tarball, a vendored source drop — is a
//! legitimate build, not a broken one, so an unresolvable revision reports `"unknown"` rather
//! than failing. Naming the exact core build is a support convenience; it is never load-bearing.

use std::process::Command;

/// The value reported when git metadata is unavailable.
const UNKNOWN: &str = "unknown";

fn main() {
    println!("cargo::rustc-env=SPIDOLA_GIT_REVISION={}", revision());
    // Without this, a rebuild after a commit would keep the stale revision baked in from the
    // last build: cargo has no other reason to believe this script's output changed.
    println!("cargo::rerun-if-changed=../../.git/HEAD");
    println!("cargo::rerun-if-env-changed=SPIDOLA_GIT_REVISION");
}

/// The short revision, marked `-dirty` when the working tree has uncommitted changes, so a
/// build made from modified sources can never be mistaken for the commit it started from.
fn revision() -> String {
    // An explicit override wins: a tarball/packaging build can stamp the revision it was cut
    // from even with no `.git` present.
    if let Ok(preset) = std::env::var("SPIDOLA_GIT_REVISION")
        && !preset.is_empty()
    {
        return preset;
    }
    let Some(short) = git(&["rev-parse", "--short", "HEAD"]) else {
        return UNKNOWN.to_owned();
    };
    // `--quiet` exits non-zero exactly when the tree is dirty; no output either way.
    let dirty = Command::new("git")
        .args(["diff", "--quiet", "HEAD"])
        .status()
        .is_ok_and(|status| !status.success());
    if dirty {
        format!("{short}-dirty")
    } else {
        short
    }
}

/// Runs a git command, returning its trimmed stdout, or `None` if git is absent, fails, or
/// prints something unusable.
fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}
