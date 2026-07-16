#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Spidola contributors
# SPDX-License-Identifier: AGPL-3.0-or-later
#
# Derives the NDK cross-compilation environment for one ABI, and asserts the NDK pin
# (TECH_SPEC §9: "build scripts assert them"; the pin itself lives in docs/toolchains.md).
#
# Sourced, never executed: every function here exports or prints, none of them build.

# The NDK pinned by docs/toolchains.md. A different NDK produces a different libmpv.so, so
# this is asserted rather than discovered — an unpinned native toolchain is how a build
# stops being reproducible without anyone noticing.
readonly NDK_VERSION="28.2.13676358"

# minSdk from apps/androidtv/gradle/libs.versions.toml. The native ABI level must match the
# Kotlin module's minSdk or the .so will refuse to load on a device the app claims to support.
readonly ANDROID_API=26

readonly ABIS=(arm64-v8a armeabi-v7a x86_64)

fail() {
  echo "build-libmpv-android: $*" >&2
  exit 1
}

# Locates the pinned NDK, honouring ANDROID_NDK_HOME first (what CI sets) and falling back
# to the standard SDK layout under ANDROID_HOME/ANDROID_SDK_ROOT.
ndk_root() {
  local candidate="${ANDROID_NDK_HOME:-}"
  if [ -z "$candidate" ]; then
    local sdk="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-}}"
    [ -n "$sdk" ] || fail "set ANDROID_NDK_HOME, or ANDROID_HOME/ANDROID_SDK_ROOT with ndk/$NDK_VERSION installed"
    candidate="$sdk/ndk/$NDK_VERSION"
  fi
  [ -d "$candidate" ] || fail "NDK not found at $candidate (pin: $NDK_VERSION)"

  # source.properties carries the real revision; the directory name is only a convention and
  # can lie after a manual move.
  local props="$candidate/source.properties"
  [ -f "$props" ] || fail "$candidate is not an NDK (no source.properties)"
  local have
  have="$(awk -F' *= *' '/^Pkg.Revision/{print $2}' "$props")"
  [ "$have" = "$NDK_VERSION" ] || fail "NDK $NDK_VERSION required (docs/toolchains.md), found $have at $candidate"

  printf '%s' "$candidate"
}

# The NDK ships one prebuilt host toolchain per platform. On Apple silicon the tag is still
# darwin-x86_64 (it runs under Rosetta), so this resolves by what exists rather than by uname.
ndk_host_tag() {
  local ndk="$1" tag
  for tag in darwin-arm64 darwin-x86_64 linux-x86_64; do
    [ -d "$ndk/toolchains/llvm/prebuilt/$tag" ] && { printf '%s' "$tag"; return; }
  done
  fail "no prebuilt NDK toolchain under $ndk/toolchains/llvm/prebuilt"
}

# The clang target triple. armeabi-v7a is the odd one: its compiler driver is prefixed
# armv7a-linux-androideabi while its sysroot directory is arm-linux-androideabi.
abi_clang_triple() {
  case "$1" in
    arm64-v8a) printf 'aarch64-linux-android' ;;
    armeabi-v7a) printf 'armv7a-linux-androideabi' ;;
    x86_64) printf 'x86_64-linux-android' ;;
    *) fail "unknown ABI: $1" ;;
  esac
}

# The sysroot directory holding the ABI's shared C++ runtime. This differs from the compiler
# triple only for armeabi-v7a.
abi_sysroot_triple() {
  case "$1" in
    arm64-v8a) printf 'aarch64-linux-android' ;;
    armeabi-v7a) printf 'arm-linux-androideabi' ;;
    x86_64) printf 'x86_64-linux-android' ;;
    *) fail "unknown ABI: $1" ;;
  esac
}

# What FFmpeg's configure calls this architecture (--arch).
abi_ffmpeg_arch() {
  case "$1" in
    arm64-v8a) printf 'aarch64' ;;
    armeabi-v7a) printf 'arm' ;;
    x86_64) printf 'x86_64' ;;
    *) fail "unknown ABI: $1" ;;
  esac
}

# What meson's [host_machine] calls this architecture: cpu_family<TAB>cpu.
# Newline-terminated on purpose: the caller consumes this with `read`, which reports EOF
# (non-zero) on an unterminated line and would trip `set -e` despite having read the values.
abi_meson_cpu() {
  case "$1" in
    arm64-v8a) printf 'aarch64\taarch64\n' ;;
    armeabi-v7a) printf 'arm\tarmv7a\n' ;;
    x86_64) printf 'x86_64\tx86_64\n' ;;
    *) fail "unknown ABI: $1" ;;
  esac
}

# Exports the cross environment for $1 (ABI) against prefix $2. Every component script runs
# under this, so the compiler identity is decided in exactly one place.
export_cross_env() {
  local abi="$1" prefix="$2"
  local ndk host_tag bin triple
  ndk="$(ndk_root)" || exit 1
  host_tag="$(ndk_host_tag "$ndk")" || exit 1
  bin="$ndk/toolchains/llvm/prebuilt/$host_tag/bin"
  triple="$(abi_clang_triple "$abi")"

  export NDK_ROOT="$ndk"
  export NDK_BIN="$bin"
  export CC="$bin/${triple}${ANDROID_API}-clang"
  export CXX="$bin/${triple}${ANDROID_API}-clang++"
  export AR="$bin/llvm-ar"
  export RANLIB="$bin/llvm-ranlib"
  export STRIP="$bin/llvm-strip"
  export NM="$bin/llvm-nm"
  export LD="$bin/ld.lld"
  export PREFIX="$prefix"

  [ -x "$CC" ] || fail "no compiler at $CC (ABI $abi, API $ANDROID_API)"

  # Point pkg-config exclusively at our own prefix. LIBDIR (not PATH) so the host's
  # /opt/homebrew/lib/pkgconfig can never satisfy a dependency — silently linking a macOS
  # .dylib's metadata into an Android build fails late and confusingly.
  export PKG_CONFIG_LIBDIR="$prefix/lib/pkgconfig"
  export PKG_CONFIG_SYSROOT_DIR=""
  export PKG_CONFIG_PATH=""
}

# Writes a meson cross file for $1 (ABI) to $2. Six of the eight components build with
# meson, so this file is the bulk of the cross-compilation contract.
write_meson_cross_file() {
  local abi="$1" out="$2"
  local cpu_family cpu
  IFS=$'\t' read -r cpu_family cpu < <(abi_meson_cpu "$abi")

  # Two things the generated file below cannot explain about itself:
  #
  # 1. -Wl,-z,max-page-size=16384 — Android 15+ requires 16 KB-aligned shared libraries; a
  #    device with 16 KB pages refuses to load a 4 KB-aligned .so outright.
  #
  # 2. There is deliberately no sys_root property. Meson turns that into
  #    PKG_CONFIG_SYSROOT_DIR, which makes pkg-config prepend the sysroot to every -I and -L
  #    it reports — including for the libraries we install into our own prefix. That yields
  #    paths like <ndk-sysroot>/<our-prefix>/include/freetype2, which do not exist, so
  #    harfbuzz stops finding the freetype built one step earlier. The clang driver already
  #    knows its sysroot from its target triple, so nothing needs the property.
  #
  # This heredoc is unquoted so the toolchain paths interpolate, which also means backticks
  # and $(...) inside it would execute. Keep prose out here, where it is inert.
  cat >"$out" <<EOF
# Generated by toolchain.sh — do not edit; regenerate via build.sh.
[binaries]
c = '$CC'
cpp = '$CXX'
ar = '$AR'
ranlib = '$RANLIB'
strip = '$STRIP'
nm = '$NM'
pkg-config = 'pkg-config'

[built-in options]
c_args = ['-fPIC', '-O2']
cpp_args = ['-fPIC', '-O2']
c_link_args = ['-Wl,-z,max-page-size=16384', '-lc++_shared']
cpp_link_args = ['-Wl,-z,max-page-size=16384']
prefix = '$PREFIX'
libdir = 'lib'

[properties]
# Point pkg-config at our prefix and nothing else. See write_meson_cross_file above for why
# there is deliberately no sys_root property here.
pkg_config_libdir = '$PREFIX/lib/pkgconfig'

[host_machine]
system = 'android'
cpu_family = '$cpu_family'
cpu = '$cpu'
endian = 'little'
EOF
}
