#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Spidola contributors
# SPDX-License-Identifier: AGPL-3.0-or-later
#
# Builds LGPL libmpv for the Android ABIs, one shared libmpv.so per ABI.
#
# Usage:
#   build.sh                    # every ABI in toolchain.sh's ABIS
#   build.sh arm64-v8a          # one ABI (what you want while iterating)
#   build.sh arm64-v8a x86_64
#
# Outputs: dist/<abi>/libmpv.so + dist/checksums.sha256
# Prerequisites and rationale: see README.md.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./toolchain.sh
source "$here/toolchain.sh"

readonly SRC_ROOT="$here/src"
readonly DIST="$here/dist"

# The dependency order. This is a DAG flattened by hand because it is short, stable, and
# reading it top-to-bottom is the fastest way to understand the build:
#   mbedtls    → ffmpeg (https)
#   freetype   → harfbuzz → libass
#   fribidi    → libass
#   libass + ffmpeg + libplacebo → mpv
readonly COMPONENTS=(mbedtls freetype harfbuzz fribidi libass ffmpeg libplacebo mpv)

require_host_tools() {
  local missing=()
  local tool
  for tool in meson ninja cmake pkg-config nasm python3 git curl; do
    command -v "$tool" >/dev/null 2>&1 || missing+=("$tool")
  done
  [ ${#missing[@]} -eq 0 ] || fail "missing host tools: ${missing[*]}
Install them first — see README.md 'Host prerequisites'."
}

build_abi() {
  local abi="$1"
  local prefix="$here/build/$abi/prefix"
  local build_root="$here/build/$abi/obj"
  mkdir -p "$prefix" "$build_root"

  export ABI="$abi"
  export SRC_ROOT BUILD_ROOT="$build_root" DIST
  export_cross_env "$abi" "$prefix"

  export CROSS_FILE="$here/build/$abi/meson-cross.ini"
  write_meson_cross_file "$abi" "$CROSS_FILE"

  echo
  echo "############ $abi ############"
  local component
  for component in "${COMPONENTS[@]}"; do
    "$here/components/$component.sh"
  done
}

write_manifest() {
  [ -d "$DIST" ] || return 0
  # Recorded so Gradle (and a reviewer) can tell a rebuilt .so from a substituted one. This
  # is the *output* manifest; sources.lock pins the inputs.
  ( cd "$DIST" && find . -name '*.so' -type f | sort | while read -r so; do
      if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$so"
      else
        shasum -a 256 "$so"
      fi
    done ) >"$DIST/checksums.sha256"
  echo
  echo "==> artifact manifest: $DIST/checksums.sha256"
  cat "$DIST/checksums.sha256"
}

main() {
  require_host_tools
  local abis=("$@")
  [ ${#abis[@]} -gt 0 ] || abis=("${ABIS[@]}")

  echo "==> NDK $NDK_VERSION at $(ndk_root)"
  "$here/fetch.sh"

  local abi
  for abi in "${abis[@]}"; do
    build_abi "$abi"
  done

  write_manifest
  echo
  echo "build: done"
}

main "$@"
