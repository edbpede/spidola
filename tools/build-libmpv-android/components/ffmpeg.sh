#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Spidola contributors
# SPDX-License-Identifier: AGPL-3.0-or-later
#
# FFmpeg, configured **LGPL**. This file is the licensing crux of the whole engine, so read
# the flag block below before changing anything in it.
#
# Contract (exported by build.sh): ABI, PREFIX, SRC_ROOT, BUILD_ROOT, and the cross env
# (CC/CXX/AR/RANLIB/NM/STRIP) from toolchain.sh.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../toolchain.sh
source "$here/../toolchain.sh"

src="$SRC_ROOT/ffmpeg"
build="$BUILD_ROOT/ffmpeg"
mkdir -p "$build"
cd "$build"

arch="$(abi_ffmpeg_arch "$ABI")"

# ── The license flags. Do not "simplify" these away. ────────────────────────
#
# PRD §10 and TECH_SPEC §12 require an LGPL FFmpeg: GPL components would make the bundled
# binary GPL, which breaks the App Store posture the project committed to and is a licence
# term the project has no authority to impose on FFmpeg's authors' behalf.
#
# --disable-gpl and --disable-nonfree are the *defaults* in FFmpeg's configure; they are
# passed explicitly anyway because an explicit flag is greppable, and verify-pins.sh greps
# for exactly this. A silent default is not an assertion — it is a coincidence that a future
# edit can revoke without leaving a trace in review.
#
# --enable-version3 makes the result **LGPL version 3 or later** instead of LGPL v2.1. This
# is forced by the TLS provider and is not a free choice:
#
#   Mbed-TLS is Apache-2.0. Apache-2.0 is incompatible with LGPL v2.1 but compatible with
#   LGPL v3, so FFmpeg lists mbedtls in EXTERNAL_LIBRARY_VERSION3_LIST (configure:1885-1893)
#   and refuses to configure without version3 (configure:4508). OpenSSL 3 is Apache-2.0 too
#   and hits the identical rule (configure:7126). The only mainstream TLS provider outside
#   that list is GnuTLS — and GnuTLS needs nettle and GMP, and GMP is LGPL v3 itself, so
#   that route arrives at v3 anyway after three more cross-compiled components.
#
# What this does and does not change:
#   - Still LGPL, still not GPL, still not nonfree. The PRD §10 requirement is "no GPL-only
#     FFmpeg components", and that holds exactly as before.
#   - LGPL v3 is compatible with the project's AGPL-3.0-or-later code (PRD §10).
#   - It is Android-only. tvOS builds its own FFmpeg under tools/build-mpvkit, and the
#     LGPLv3-vs-App-Store question that makes v2.1 attractive on Apple does not arise on
#     Play. That decision belongs to build-mpvkit and is deliberately not made here.
#
# The resulting license string is asserted after configure, below: flags state intent,
# config.h states fact, and only the second one is evidence.
license_flags=(
  --disable-gpl
  --disable-nonfree
  --enable-version3
  # libpostproc is GPL-only upstream. Naming it here means that if the licence flags above
  # were ever loosened, postproc still could not be pulled in without a second deliberate
  # edit that review would see.
  --disable-postproc
)

cross_flags=(
  --enable-cross-compile
  --target-os=android
  --arch="$arch"
  --sysroot="$NDK_ROOT/toolchains/llvm/prebuilt/$(ndk_host_tag "$NDK_ROOT")/sysroot"
  --cc="$CC"
  --cxx="$CXX"
  --ar="$AR"
  --ranlib="$RANLIB"
  --nm="$NM"
  --strip="$STRIP"
  --pkg-config=pkg-config
  --pkg-config-flags=--static
)

# Static libs linked into the one shared libmpv.so (see README "One .so, not nine").
build_flags=(
  --enable-static
  --disable-shared
  --enable-pic
  --enable-optimizations
  --disable-programs
  --disable-doc
  --disable-avdevice
  --disable-debug
)

# What the engine actually needs. mpv is the codec-breadth engine (TECH_SPEC §8), so the
# default demuxer/decoder set stays on — trimming it would defeat the reason this engine
# exists. Only the platform integrations are named.
feature_flags=(
  # https for IPTV sources. mbedtls rather than OpenSSL — see sources.lock.
  --enable-mbedtls
  --enable-protocol=https,http,tls,file,tcp,udp,rtp,hls,crypto
  # MediaCodec hardware decoding: the difference between watchable and not on a
  # Chromecast-class device (PRD §9's low-end baseline).
  --enable-jni
  --enable-mediacodec
  --disable-vulkan
)

if [ "$ABI" = "armeabi-v7a" ]; then
  # 32-bit ARM: NEON is not implied by the baseline, and without it software decode on the
  # low-end baseline is not viable.
  feature_flags+=(--enable-neon)
fi

echo "==> configuring ffmpeg ($ABI, LGPL)"
"$src/configure" \
  --prefix="$PREFIX" \
  "${license_flags[@]}" \
  "${cross_flags[@]}" \
  "${build_flags[@]}" \
  "${feature_flags[@]}"

# ── The assertion that matters ──────────────────────────────────────────────
# configure resolves the licence from the flags and writes the answer into config.h
# (configure:8219 emits FFMPEG_LICENSE; the resolution logic is at configure:4515-4524).
# Asserting the *output* catches what grepping the *input* cannot: a flag we forgot, a flag
# upstream renamed, or a dependency that silently forced the licence up.
want_license='#define FFMPEG_LICENSE "LGPL version 3 or later"'
if ! grep -qxF "$want_license" "$build/config.h"; then
  echo "error: ffmpeg did not configure as LGPL (PRD §10, TECH_SPEC §12)." >&2
  echo "       expected: $want_license" >&2
  echo "       config.h says: $(grep -F 'FFMPEG_LICENSE' "$build/config.h" || echo '(no FFMPEG_LICENSE line)')" >&2
  exit 1
fi
echo "==> ffmpeg license asserted: LGPL version 3 or later"

make -j"$(getconf _NPROCESSORS_ONLN)"
make install
