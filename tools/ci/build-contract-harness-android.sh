#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Spidola contributors
# SPDX-License-Identifier: AGPL-3.0-or-later
#
# Builds and runs the Kotlin FFI contract harness (TECH_SPEC §5, §10) against the host-arch
# `core-api` library. The harness links the real compiled core through the generated Kotlin
# (JNA) bindings and drives the same fixture flow as the Rust and Swift harnesses — proving
# parity. Run from the Android CI lane (needs a `kotlinc` on PATH and a JDK); the native library
# is loaded by JNA via `-Djna.library.path`.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"

generated="apps/androidtv/core/corekit/generated/uniffi/core_api/core_api.kt"
harness="apps/androidtv/contract-harness/ContractHarness.kt"
libdir="target/debug"
work="target/contract-harness-android"
jars="$work/jars"
mkdir -p "$jars"

coroutines_version="1.11.0"
jna_version="5.17.0"
central="https://repo1.maven.org/maven2"

fetch() { # url dest
  if [ ! -f "$2" ]; then curl -fsSL "$1" -o "$2"; fi
}

echo "== building core-api cdylib (host) =="
cargo build -p core-api --lib

echo "== regenerating bindings (must match committed) =="
cargo run --quiet -p xtask -- check-bindings

echo "== resolving JNA + coroutines jars =="
fetch "$central/net/java/dev/jna/jna/$jna_version/jna-$jna_version.jar" "$jars/jna.jar"
fetch "$central/org/jetbrains/kotlinx/kotlinx-coroutines-core-jvm/$coroutines_version/kotlinx-coroutines-core-jvm-$coroutines_version.jar" "$jars/coroutines.jar"

kotlin_home="$(dirname "$(dirname "$(command -v kotlinc)")")"
stdlib="$kotlin_home/lib/kotlin-stdlib.jar"
deps="$jars/jna.jar:$jars/coroutines.jar"

echo "== compiling Kotlin harness =="
kotlinc -classpath "$deps" -include-runtime -d "$work/harness.jar" "$generated" "$harness"

echo "== running Kotlin harness =="
java -Djna.library.path="$root/$libdir" \
  -classpath "$work/harness.jar:$stdlib:$deps" \
  dev.spidola.tv.contract.ContractHarnessKt
