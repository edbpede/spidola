// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The `uniffi-bindgen` helper binary (TECH_SPEC §5).
//!
//! UniFFI's proc-macro mode has no standalone bindgen CLI on crates.io; the recommended
//! pattern is a tiny in-repo binary that forwards to `uniffi::uniffi_bindgen_main`, pinned
//! to the exact `uniffi` version the core links, so the generator and the runtime can never
//! drift. `xtask gen-bindings` / `xtask check-bindings` invoke this in **library mode**
//! against the compiled `core-api` cdylib.
#![forbid(unsafe_code)]

fn main() {
    uniffi::uniffi_bindgen_main();
}
