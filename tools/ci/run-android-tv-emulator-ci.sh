#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Spidola contributors
# SPDX-License-Identifier: AGPL-3.0-or-later

set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
evidence_dir="$root/target/androidtv-m0"
system_image="system-images;android-36;android-tv;x86_64"
avd_name="spidola-android-tv-ci"
emulator_port=5554
serial="emulator-$emulator_port"
emulator_pid=""
emulator_bin="${ANDROID_SDK_ROOT:?ANDROID_SDK_ROOT must be set}/emulator/emulator"

cleanup() {
    adb -s "$serial" emu kill >/dev/null 2>&1 || true
    if [ -n "$emulator_pid" ]; then
        kill "$emulator_pid" 2>/dev/null || true
    fi
}
trap cleanup EXIT

mkdir -p "$evidence_dir"

sdkmanager --install emulator "$system_image"
printf 'no\n' | avdmanager create avd \
    --force \
    --name "$avd_name" \
    --package "$system_image" \
    --device tv_1080p

adb start-server
"$emulator_bin" \
    -port "$emulator_port" \
    -avd "$avd_name" \
    -no-window \
    -gpu swiftshader_indirect \
    -no-snapshot \
    -noaudio \
    -no-boot-anim \
    > "$evidence_dir/emulator.log" 2>&1 &
emulator_pid=$!

booted=false
for _ in {1..300}; do
    if ! kill -0 "$emulator_pid" 2>/dev/null; then
        echo "Android TV emulator exited before completing boot." >&2
        tail -n 200 "$evidence_dir/emulator.log" >&2
        exit 1
    fi

    if [ "$(adb -s "$serial" shell getprop sys.boot_completed 2>/dev/null || true)" = "1" ]; then
        booted=true
        break
    fi
    sleep 2
done

if [ "$booted" != true ]; then
    echo "Android TV emulator did not complete boot within 600 seconds." >&2
    tail -n 200 "$evidence_dir/emulator.log" >&2
    exit 1
fi

"$root/tools/ci/run-android-tv-emulator-smoke.sh"
