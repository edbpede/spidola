#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Spidola contributors
# SPDX-License-Identifier: AGPL-3.0-or-later
#
# Builds and runs the Swift FFI contract harness (TECH_SPEC §5, §10) against the host-arch
# `core-api` library. The harness links the real compiled core through the generated Swift
# bindings and drives the same fixture flow as the Rust and Kotlin harnesses — proving parity.
# Run from the Apple CI lane on a macOS runner; also runnable locally with a Swift toolchain.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"

generated="apps/tvos/Packages/CoreKit/Generated"
harness="apps/tvos/contract-harness/main.swift"
libdir="target/debug"
out="target/contract-harness-tvos"

echo "== building core-api cdylib (host) =="
cargo build -p core-api --lib

echo "== regenerating bindings (must match committed) =="
cargo run --quiet -p xtask -- check-bindings

echo "== compiling Swift harness =="
swiftc -O \
  -o "$out" \
  "$generated/core_api.swift" "$harness" \
  -I "$generated" \
  -Xcc -fmodule-map-file="$generated/core_apiFFI.modulemap" \
  -L "$libdir" -lcore_api \
  -Xlinker -rpath -Xlinker "$root/$libdir"

echo "== running Swift harness =="
"$out"
