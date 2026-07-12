// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Shared workspace-path and Cargo-invocation helpers for the packaging tasks.

use std::path::{Path, PathBuf};

/// The workspace root, derived from this crate's compile-time manifest directory
/// (`<root>/crates/xtask`).
pub(crate) fn workspace_root() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(Path::parent)
        .map_or_else(|| manifest.to_path_buf(), Path::to_path_buf)
}

/// The Cargo target directory, honouring `CARGO_TARGET_DIR`.
pub(crate) fn target_dir(root: &Path) -> PathBuf {
    std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| root.join("target"), PathBuf::from)
}

/// The `cargo` executable, honouring the `CARGO` env var Cargo sets for its subcommands.
pub(crate) fn cargo() -> String {
    std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned())
}
