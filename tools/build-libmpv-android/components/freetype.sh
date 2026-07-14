#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Spidola contributors
# SPDX-License-Identifier: AGPL-3.0-or-later
#
# FreeType — the font rasterizer under libass' subtitle rendering.
#
# Contract (exported by build.sh): ABI, PREFIX, SRC_ROOT, BUILD_ROOT, CROSS_FILE.
set -euo pipefail

echo "==> building freetype ($ABI)"

# harfbuzz=disabled breaks the FreeType↔HarfBuzz dependency cycle (each can optionally use
# the other). Upstream's answer is to build FreeType twice — once without HarfBuzz, then
# again against it. We build it once, without.
#
# What that costs, precisely: FreeType's auto-hinter loses HarfBuzz-assisted glyph coverage
# analysis, which slightly degrades auto-hinted rendering of complex scripts. It does NOT
# affect shaping — libass links HarfBuzz directly and does its own. On a 10-foot TV UI at
# subtitle sizes the difference is not visible, and the second pass would double this
# component's build time and add an ordering constraint that is easy to get subtly wrong.
# If complex-script subtitle hinting is ever reported as inadequate, the two-pass build is
# the fix, and it belongs here.
#
# png/brotli/bzip2 are for colour-emoji and WOFF2 fonts, neither of which subtitle
# rendering loads. zlib comes from the NDK sysroot.
meson setup "$BUILD_ROOT/freetype" "$SRC_ROOT/freetype" \
  --cross-file "$CROSS_FILE" \
  --prefix "$PREFIX" \
  --buildtype release \
  --default-library static \
  -Dharfbuzz=disabled \
  -Dpng=disabled \
  -Dbrotli=disabled \
  -Dbzip2=disabled \
  -Dzlib=system \
  -Dtests=disabled

meson compile -C "$BUILD_ROOT/freetype"
meson install -C "$BUILD_ROOT/freetype" --no-rebuild
