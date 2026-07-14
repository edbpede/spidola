#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Spidola contributors
# SPDX-License-Identifier: AGPL-3.0-or-later
#
# HarfBuzz — the text shaper libass calls directly for complex scripts.
#
# Contract (exported by build.sh): ABI, PREFIX, SRC_ROOT, BUILD_ROOT, CROSS_FILE.
set -euo pipefail

echo "==> building harfbuzz ($ABI)"

# freetype=enabled: this is the live half of the FreeType↔HarfBuzz cycle — HarfBuzz reads
# font tables through FreeType, which freetype.sh has already installed into $PREFIX.
#
# Everything else is disabled explicitly rather than left on meson's `auto`. On `auto`,
# meson probes and a host-installed glib or cairo under /opt/homebrew can be picked up into
# an Android build; the failure lands at link time, far from the cause. `utilities` and
# `tests` additionally try to build host executables, which a cross build cannot run.
meson setup "$BUILD_ROOT/harfbuzz" "$SRC_ROOT/harfbuzz" \
  --cross-file "$CROSS_FILE" \
  --prefix "$PREFIX" \
  --buildtype release \
  --default-library static \
  -Dfreetype=enabled \
  -Dglib=disabled \
  -Dgobject=disabled \
  -Dcairo=disabled \
  -Dicu=disabled \
  -Dchafa=disabled \
  -Dgraphite2=disabled \
  -Dtests=disabled \
  -Ddocs=disabled \
  -Dintrospection=disabled \
  -Dutilities=disabled

meson compile -C "$BUILD_ROOT/harfbuzz"
meson install -C "$BUILD_ROOT/harfbuzz" --no-rebuild
