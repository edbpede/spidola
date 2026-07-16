#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Spidola contributors
# SPDX-License-Identifier: AGPL-3.0-or-later
#
# mpv, configured **LGPL**, built as the single shared libmpv.so the engine loads.
#
# Contract (exported by build.sh): ABI, PREFIX, SRC_ROOT, BUILD_ROOT, CROSS_FILE, DIST.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../toolchain.sh
source "$here/../toolchain.sh"

src="$SRC_ROOT/mpv"
build="$BUILD_ROOT/mpv"

# ── The license flag ────────────────────────────────────────────────────────
#
# `-Dgpl=false` is mpv's LGPL switch. It is NOT `--enable-lgpl`: that was the waf-era flag,
# and mpv deleted waf in 0.36 when it moved to meson. mpv declares itself
# `license: ['GPL2+', 'LGPL2.1+']` (meson.build:3) and `-Dgpl=false` selects the LGPL half,
# which makes every GPL-only component (cdda, dvbin, dvdnav — see meson.build:612-636) a
# hard configure error rather than a silent inclusion.
#
# Asserted after setup against mpv's own config.h, for the same reason as ffmpeg.sh: the
# flag is the intent, the generated define is the evidence.
license_flags=(-Dgpl=false)

# libmpv only — the CLI player is not shippable inside an app and would only add surface.
output_flags=(
  -Dlibmpv=true
  -Dcplayer=false
  -Dmanpage-build=disabled
  -Dhtml-build=disabled
  -Dtests=false
  # A compile timestamp would make two builds of the same pins differ byte-for-byte, which
  # is exactly what the checksum manifest is supposed to detect.
  -Dbuild-date=false
)

# Android platform integration. `egl-android` gives vo=gpu a context on the SurfaceView the
# engine hands over; `android-media-ndk` is mpv's side of MediaCodec hardware decode.
android_flags=(
  -Degl-android=enabled
  -Dandroid-media-ndk=enabled
  -Daudiotrack=enabled
  -Dopensles=enabled
  -Dplain-gl=enabled
  -Dvulkan=disabled
  # Bionic ships no iconv; leaving this on `auto` lets a stray host header decide.
  -Diconv=disabled
  # Lua drives mpv's scripting layer. The engine drives mpv through the client API only, so
  # scripting is surface with no consumer — and an interpreter we would have to bundle.
  -Dlua=disabled
)

echo "==> configuring mpv ($ABI, LGPL)"
rm -rf "$build"
meson setup "$build" "$src" \
  --cross-file "$CROSS_FILE" \
  --prefix "$PREFIX" \
  --buildtype release \
  --default-library shared \
  "${license_flags[@]}" \
  "${output_flags[@]}" \
  "${android_flags[@]}"

# mpv turns each feature into a HAVE_<FEATURE> define (meson.build:1729-1730), so a
# not-GPL build is provable from the generated header rather than from our own flags.
if ! grep -qE '^#define HAVE_GPL 0$' "$build/config.h"; then
  echo "error: mpv did not configure as LGPL (PRD §10, TECH_SPEC §12)." >&2
  echo "       config.h says: $(grep -E '#define HAVE_GPL' "$build/config.h" || echo '(no HAVE_GPL line)')" >&2
  exit 1
fi
echo "==> mpv license asserted: HAVE_GPL 0 (LGPL 2.1 or later)"

meson compile -C "$build"
meson install -C "$build" --no-rebuild

mkdir -p "$DIST/$ABI"
so="$PREFIX/lib/libmpv.so"
[ -f "$so" ] || fail "mpv built but $so is missing"

# Strip: debug info is ~10x the shipped size and the app has no symbolizer for it.
"$STRIP" --strip-unneeded "$so" -o "$DIST/$ABI/libmpv.so"
echo "==> $DIST/$ABI/libmpv.so ($(du -h "$DIST/$ABI/libmpv.so" | cut -f1))"

# The client API headers, exported once (they are ABI-independent) so the engine's JNI shim
# has something to compile against without reaching into a per-ABI build tree.
mkdir -p "$DIST/include"
cp -R "$PREFIX/include/mpv" "$DIST/include/"
# The JNI shim registers Android's JavaVM through FFmpeg before mpv creates a video output.
# Stage only that narrow public header instead of exposing the entire FFmpeg development surface.
mkdir -p "$DIST/include/libavcodec"
cp "$PREFIX/include/libavcodec/jni.h" "$DIST/include/libavcodec/jni.h"
