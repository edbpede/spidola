#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Spidola contributors
# SPDX-License-Identifier: AGPL-3.0-or-later
#
# libplacebo — mpv's GPU rendering library. Not optional: mpv 0.41.0 declares it a hard
# dependency at >= 6.338.2 (mpv meson.build:29) and sets features['libplacebo'] = true
# unconditionally (line 51). vo=gpu does not exist without it.
#
# Contract (exported by build.sh): ABI, PREFIX, SRC_ROOT, BUILD_ROOT, CROSS_FILE.
set -euo pipefail

echo "==> building libplacebo ($ABI)"

# opengl=enabled / vulkan=disabled: Android TV's reliable GPU path is GLES via EGL, which is
# what mpv's context_android.c targets (mpv meson.build:1235). Enabling Vulkan would also
# drag in 3rdparty/Vulkan-Headers, which fetch.sh deliberately does not pin.
#
# glslang/shaderc disabled: those compile GLSL to SPIR-V, which only the Vulkan/D3D11
# backends need. The GL backend emits GLSL and hands it to the driver, so the shader
# compiler is dead weight — and shaderc alone would roughly double this build.
#
# demos=false keeps 3rdparty/nuklear out, matching what fetch.sh pins.
meson setup "$BUILD_ROOT/libplacebo" "$SRC_ROOT/libplacebo" \
  --cross-file "$CROSS_FILE" \
  --prefix "$PREFIX" \
  --buildtype release \
  --default-library static \
  -Dopengl=enabled \
  -Dgl-proc-addr=disabled \
  -Dvulkan=disabled \
  -Dd3d11=disabled \
  -Dglslang=disabled \
  -Dshaderc=disabled \
  -Dlcms=disabled \
  -Dlibdovi=disabled \
  -Dxxhash=disabled \
  -Ddemos=false \
  -Dtests=false

meson compile -C "$BUILD_ROOT/libplacebo"
meson install -C "$BUILD_ROOT/libplacebo" --no-rebuild
