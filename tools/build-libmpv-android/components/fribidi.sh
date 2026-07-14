#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Spidola contributors
# SPDX-License-Identifier: AGPL-3.0-or-later
#
# FriBidi — the Unicode bidirectional algorithm libass uses for RTL subtitles.
#
# Contract (exported by build.sh): ABI, PREFIX, SRC_ROOT, BUILD_ROOT, CROSS_FILE.
set -euo pipefail

echo "==> building fribidi ($ABI)"

# bin=false matters for a cross build specifically: FriBidi's CLI would be built for
# Android and could not run on the build host anyway.
meson setup "$BUILD_ROOT/fribidi" "$SRC_ROOT/fribidi" \
  --cross-file "$CROSS_FILE" \
  --prefix "$PREFIX" \
  --buildtype release \
  --default-library static \
  -Dbin=false \
  -Ddocs=false \
  -Dtests=false

meson compile -C "$BUILD_ROOT/fribidi"
meson install -C "$BUILD_ROOT/fribidi" --no-rebuild
