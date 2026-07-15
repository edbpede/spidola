#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Spidola contributors
# SPDX-License-Identifier: AGPL-3.0-or-later

# Keep test execution, evidence capture, and exit-status propagation in one process so a failed
# instrumentation test still retains its logcat and final-screen evidence.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
evidence_dir="$root/target/androidtv-m0"

serial="${ANDROID_SERIAL:-}"
if [[ -z "$serial" ]]; then
    readarray -t connected_serials < <(adb devices | awk 'NR > 1 && $2 == "device" { print $1 }')
    if [[ "${#connected_serials[@]}" -ne 1 ]]; then
        echo "expected exactly one connected Android emulator; set ANDROID_SERIAL explicitly" >&2
        exit 2
    fi
    serial="${connected_serials[0]}"
fi
if [[ "$serial" != emulator-* ]]; then
    echo "refusing to clear app data on non-emulator device: $serial" >&2
    exit 2
fi
if [[ "$(adb -s "$serial" get-state 2>/dev/null)" != "device" ]]; then
    echo "Android emulator is not connected: $serial" >&2
    exit 2
fi
export ANDROID_SERIAL="$serial"

mkdir -p "$evidence_dir"
cd "$root/apps/androidtv"

adb -s "$serial" logcat -c || echo "warning: adb logcat -c failed; continuing with uncleared buffer" >&2

test_status=0
./gradlew :app:connectedDebugAndroidTest || test_status=$?

evidence_status=0
adb -s "$serial" logcat -d -v threadtime > "$evidence_dir/logcat.txt" || evidence_status=$?
adb -s "$serial" exec-out screencap -p > "$evidence_dir/final-screen.png" || evidence_status=$?

if [ "$test_status" -ne 0 ]; then
    exit "$test_status"
fi

exit "$evidence_status"
