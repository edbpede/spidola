// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! `xtask` — developer automation: UniFFI binding generation, XCFramework / cargo-ndk
//! packaging, fixture refresh, and release tasks (Phase 2+). The cargo-xtask pattern,
//! not shell scripts (TECH_SPEC §3.3).
#![forbid(unsafe_code)]

mod bindings;
mod packaging;
mod paths;
mod phase1;

use anyhow::anyhow;

const USAGE: &str = "usage: cargo xtask <task>\n  \
tasks:\n  \
  phase1-verify      run the Phase 1 end-to-end pipeline verification\n  \
  gen-bindings       generate the Swift + Kotlin UniFFI bindings (library mode)\n  \
  check-bindings     fail if the committed bindings drift from the Rust definitions\n  \
  package-xcframework  build the tvOS XCFramework (device + simulator)\n  \
  package-android    build the Android per-ABI jniLibs via cargo-ndk";

fn main() -> anyhow::Result<()> {
    match std::env::args().nth(1).as_deref() {
        Some("phase1-verify") => {
            // Owns a multi-thread runtime for the end-to-end pipeline (fetch + blocking DB).
            let runtime = tokio::runtime::Runtime::new()?;
            let report = runtime.block_on(phase1::verify(50_000))?;
            print!("{report}");
            Ok(())
        }
        Some("gen-bindings") => bindings::generate(),
        Some("check-bindings") => bindings::check(),
        Some("package-xcframework") => packaging::xcframework(),
        Some("package-android") => packaging::android(),
        Some(other) => Err(anyhow!("unknown task `{other}`\n{USAGE}")),
        None => Err(anyhow!("{USAGE}")),
    }
}
