#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Spidola contributors
# SPDX-License-Identifier: AGPL-3.0-or-later
#
# libass — the ASS/SSA subtitle renderer. mpv declares it a hard dependency
# (mpv meson.build:32, and features['libass'] is unconditionally true at line 50).
#
# Contract (exported by build.sh): ABI, PREFIX, SRC_ROOT, BUILD_ROOT, CROSS_FILE.
set -euo pipefail

echo "==> building libass ($ABI)"

# require-system-font-provider=false is mandatory on Android, not a preference. It defaults
# to *true*, and libass' system font providers are fontconfig, CoreText, and DirectWrite —
# Android has none of them, so leaving the default in place fails configure outright.
#
# The consequence is real and worth naming: without a system font provider libass cannot
# resolve a font by name from the OS, so subtitle styles naming a font we do not carry fall
# back to whatever font mpv is handed. The engine's job is therefore to point mpv at a font
# directory; until it does, embedded fonts (which most ASS subtitles carry) still render.
#
# libass' rasterizer is the hot path when subtitles are on, and it ships hand-written
# assembly — but only for aarch64 and x86_64. It has none for 32-bit ARM, where meson's
# `enabled` (as opposed to `auto`) turns "no assembly for this architecture" into a hard
# configure error rather than a warning.
#
# `enabled` is still the right setting on the architectures that have it: on `auto`, a
# missing nasm would silently produce a slower libass, and "subtitles got slow on x86_64
# because a host tool was absent" is precisely the kind of thing that should fail loudly.
# So the flag tracks the architecture rather than being relaxed everywhere.
case "$ABI" in
  armeabi-v7a) asm_flag="-Dasm=disabled" ;;
  *) asm_flag="-Dasm=enabled" ;;
esac

# checkasm/compare/profile/fuzz are *executables*, and disabling them is load-bearing here
# rather than mere trimming. meson builds executables position-independent by default, the
# Nasm language has no notion of PIE, and so on x86_64 — the only ABI where libass' assembly
# is nasm — configure dies with "Language Nasm does not support position-independent
# executable" before compiling a line. They are also all host-test tooling that a cross
# build could never run.

meson setup "$BUILD_ROOT/libass" "$SRC_ROOT/libass" \
  --cross-file "$CROSS_FILE" \
  --prefix "$PREFIX" \
  --buildtype release \
  --default-library static \
  -Drequire-system-font-provider=false \
  -Dfontconfig=disabled \
  -Dcoretext=disabled \
  -Ddirectwrite=disabled \
  -Dlibunibreak=disabled \
  "$asm_flag" \
  -Dtest=disabled \
  -Dcheckasm=disabled \
  -Dcompare=disabled \
  -Dprofile=disabled \
  -Dfuzz=disabled

meson compile -C "$BUILD_ROOT/libass"
meson install -C "$BUILD_ROOT/libass" --no-rebuild
