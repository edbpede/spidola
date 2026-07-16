#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Spidola contributors
# SPDX-License-Identifier: AGPL-3.0-or-later
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
output_dir="$root/dist/android"
version="0.0.0-$(git -C "$root" rev-parse --short HEAD)"
version_code="1"
require_signing=false

usage() {
  cat <<'USAGE'
Usage: build-android-direct.sh [--version NAME] [--version-code NUMBER]
                               [--output-dir DIR] [--require-signing]

Builds one APK containing arm64-v8a, armeabi-v7a, and x86_64 plus a SHA-256 manifest.
The pinned libmpv build must already exist. Signing is optional for a dry run and mandatory
with --require-signing; configure it with the four SPIDOLA_ANDROID_* variables.
USAGE
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --version) version="${2:?missing value for --version}"; shift 2 ;;
    --version-code) version_code="${2:?missing value for --version-code}"; shift 2 ;;
    --output-dir) output_dir="${2:?missing value for --output-dir}"; shift 2 ;;
    --require-signing) require_signing=true; shift ;;
    --help|-h) usage; exit 0 ;;
    *) echo "error: unknown argument: $1" >&2; usage >&2; exit 2 ;;
  esac
done

case "$version_code" in
  ''|*[!0-9]*) echo "error: --version-code must be a positive integer" >&2; exit 2 ;;
esac
[ "$version_code" -gt 0 ] || { echo "error: --version-code must be positive" >&2; exit 2; }

signing_names=(
  SPIDOLA_ANDROID_KEYSTORE
  SPIDOLA_ANDROID_STORE_PASSWORD
  SPIDOLA_ANDROID_KEY_ALIAS
  SPIDOLA_ANDROID_KEY_PASSWORD
)
missing_signing=()
for name in "${signing_names[@]}"; do
  [ -n "${!name:-}" ] || missing_signing+=("$name")
done
if $require_signing && [ "${#missing_signing[@]}" -ne 0 ]; then
  echo "error: signed release requires: ${missing_signing[*]}" >&2
  exit 1
fi
if [ "${#missing_signing[@]}" -ne 0 ] && [ "${#missing_signing[@]}" -ne "${#signing_names[@]}" ]; then
  echo "error: release signing is partially configured; set all four variables or none" >&2
  exit 1
fi

abis=(arm64-v8a armeabi-v7a x86_64)
for abi in "${abis[@]}"; do
  [ -f "$root/tools/build-libmpv-android/dist/$abi/libmpv.so" ] || {
    echo "error: missing pinned libmpv for $abi; run tools/build-libmpv-android/build.sh" >&2
    exit 1
  }
done

"$root/tools/build-libmpv-android/verify-pins.sh"
if [ -z "${ANDROID_NDK_HOME:-${ANDROID_NDK_ROOT:-}}" ]; then
  sdk_root="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-}}"
  if [ -z "$sdk_root" ] && [ -f "$root/apps/androidtv/local.properties" ]; then
    sdk_root="$(sed -n 's/^sdk\.dir=//p' "$root/apps/androidtv/local.properties" | head -1)"
  fi
  pinned_ndk="$sdk_root/ndk/28.2.13676358"
  [ -d "$pinned_ndk" ] || {
    echo "error: set ANDROID_NDK_HOME to the pinned NDK 28.2.13676358" >&2
    exit 1
  }
  export ANDROID_NDK_HOME="$pinned_ndk"
fi
cargo run --manifest-path "$root/Cargo.toml" -p xtask -- package-android

gradle=("$root/apps/androidtv/gradlew" --project-dir "$root/apps/androidtv")
"${gradle[@]}" \
  -PspidolaVersionName="$version" \
  -PspidolaVersionCode="$version_code" \
  :app:licenseeRelease :app:assembleRelease

apk_list="$(find "$root/apps/androidtv/app/build/outputs/apk/release" -maxdepth 1 -type f -name '*.apk' | sort)"
apk_count="$(printf '%s\n' "$apk_list" | awk 'NF { count++ } END { print count + 0 }')"
[ "$apk_count" -eq 1 ] || {
  echo "error: expected one release APK, found $apk_count" >&2
  printf '  %s\n' "$apk_list" >&2
  exit 1
}
apk="$apk_list"

for abi in "${abis[@]}"; do
  for library in libcore_api.so libmpv.so libspidola_mpv.so libc++_shared.so; do
    entry="lib/$abi/$library"
    count="$(zipinfo -1 "$apk" | awk -v wanted="$entry" '$0 == wanted { count++ } END { print count + 0 }')"
    [ "$count" -eq 1 ] || { echo "error: $apk contains $count copies of $entry" >&2; exit 1; }
  done
done

if $require_signing; then
  apksigner="$(command -v apksigner || true)"
  if [ -z "$apksigner" ] && [ -n "${ANDROID_HOME:-}" ]; then
    apksigner="$(find "$ANDROID_HOME/build-tools" -type f -name apksigner 2>/dev/null | sort | tail -1)"
  fi
  [ -n "$apksigner" ] || { echo "error: apksigner is required to verify the signed APK" >&2; exit 1; }
  "$apksigner" verify --verbose "$apk"
fi

safe_version="$(printf '%s' "$version" | tr -c 'A-Za-z0-9._-' '-')"
mkdir -p "$output_dir"
output_apk="$output_dir/spidola-android-tv-$safe_version.apk"
cp "$apk" "$output_apk"
checksum_file="$output_dir/SHA256SUMS"
(
  cd "$output_dir"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "${output_apk##*/}"
  else
    shasum -a 256 "${output_apk##*/}"
  fi
) > "$checksum_file"

echo "release APK: $output_apk"
echo "checksums:   $checksum_file"
if ! $require_signing; then
  echo "note: dry-run artifact is unsigned; publication must use --require-signing"
fi
