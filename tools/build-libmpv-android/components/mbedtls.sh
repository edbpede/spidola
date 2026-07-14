#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Spidola contributors
# SPDX-License-Identifier: AGPL-3.0-or-later
#
# Mbed-TLS — the TLS provider FFmpeg links for https IPTV sources.
# The only cmake component; everything else here builds with meson.
#
# Contract (exported by build.sh): ABI, PREFIX, SRC_ROOT, BUILD_ROOT.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../toolchain.sh
source "$here/../toolchain.sh"

echo "==> building mbedtls ($ABI)"

# The NDK's own cmake toolchain file rather than a hand-written one: it is the only thing
# that reliably keeps cmake's compiler probes, ABI flags, and sysroot in agreement.
cmake -S "$SRC_ROOT/mbedtls" -B "$BUILD_ROOT/mbedtls" \
  -DCMAKE_TOOLCHAIN_FILE="$NDK_ROOT/build/cmake/android.toolchain.cmake" \
  -DANDROID_ABI="$ABI" \
  -DANDROID_PLATFORM="android-$ANDROID_API" \
  -DCMAKE_INSTALL_PREFIX="$PREFIX" \
  -DCMAKE_BUILD_TYPE=Release \
  -DCMAKE_POSITION_INDEPENDENT_CODE=ON \
  -DUSE_STATIC_MBEDTLS_LIBRARY=ON \
  -DUSE_SHARED_MBEDTLS_LIBRARY=OFF \
  -DENABLE_PROGRAMS=OFF \
  -DENABLE_TESTING=OFF

cmake --build "$BUILD_ROOT/mbedtls" -j"$(getconf _NPROCESSORS_ONLN)"
cmake --install "$BUILD_ROOT/mbedtls"
