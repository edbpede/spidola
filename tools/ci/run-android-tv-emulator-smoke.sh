#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Spidola contributors
# SPDX-License-Identifier: AGPL-3.0-or-later

# The emulator-runner action executes each newline in its `script` input in a separate shell.
# Keep test execution, evidence capture, and exit-status propagation in one process.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
evidence_dir="$root/target/androidtv-m0"

mkdir -p "$evidence_dir"
cd "$root/apps/androidtv"

adb logcat -c

test_status=0
./gradlew :app:connectedDebugAndroidTest || test_status=$?

evidence_status=0
adb logcat -d -v threadtime > "$evidence_dir/logcat.txt" || evidence_status=$?
adb exec-out screencap -p > "$evidence_dir/final-screen.png" || evidence_status=$?

if [ "$test_status" -ne 0 ]; then
    exit "$test_status"
fi

exit "$evidence_status"
