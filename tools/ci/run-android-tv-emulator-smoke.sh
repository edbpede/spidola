#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Spidola contributors
# SPDX-License-Identifier: AGPL-3.0-or-later

# Keep test execution, evidence capture, and exit-status propagation in one process so a failed
# instrumentation test still retains its logcat and final-screen evidence.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
evidence_dir="$root/target/androidtv-m0"

mkdir -p "$evidence_dir"
cd "$root/apps/androidtv"

adb logcat -c || echo "warning: adb logcat -c failed; continuing with uncleared buffer" >&2

test_status=0
./gradlew :app:connectedDebugAndroidTest || test_status=$?

evidence_status=0
adb logcat -d -v threadtime > "$evidence_dir/logcat.txt" || evidence_status=$?
adb exec-out screencap -p > "$evidence_dir/final-screen.png" || evidence_status=$?

if [ "$test_status" -ne 0 ]; then
    exit "$test_status"
fi

exit "$evidence_status"
