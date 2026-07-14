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
export ANDROID_AVD_HOME="$HOME/.android/avd"

dump_diagnostics() {
    local status="$1"
    {
        echo "=== Android TV emulator CI failure diagnostics (exit status $status) ==="
        echo "--- adb devices -l ---"
        adb devices -l || true
        echo "--- adb -s $serial get-state ---"
        adb -s "$serial" get-state || true
        echo "--- last 200 lines of emulator.log ---"
        tail -n 200 "$evidence_dir/emulator.log" 2>/dev/null || true
        echo "--- df -h ---"
        df -h || true
        echo "--- free -h ---"
        free -h || true
        echo "--- dmesg OOM traces ---"
        sudo -n dmesg 2>/dev/null | grep -iE "oom|killed process" || echo "(no OOM traces found)"
    } | tee "$evidence_dir/failure-diagnostics.txt" >&2 || true
}

cleanup() {
    adb -s "$serial" emu kill >/dev/null 2>&1 || true
    if [ -n "$emulator_pid" ]; then
        kill "$emulator_pid" 2>/dev/null || true
        # Give qemu a chance to exit gracefully before the runner's orphan-process reaper
        # mistakes a lingering process for something else (previously misread as an OOM kill).
        local waited=0
        while [ "$waited" -lt 15 ] && kill -0 "$emulator_pid" 2>/dev/null; do
            sleep 1
            waited=$((waited + 1))
        done
        kill -9 "$emulator_pid" 2>/dev/null || true
    fi
}

on_exit() {
    local status=$?
    if [ "$status" -ne 0 ]; then dump_diagnostics "$status" || true; fi
    cleanup
}
trap on_exit EXIT

# Poll adb + boot properties until they agree the device is up, requiring several consecutive
# healthy reads. This rides out the adbd restart that happens at end-of-boot, which otherwise
# transiently drops the device offline right as the first unguarded adb command in the smoke
# script would run under set -euo pipefail (previously misread as an exit-255 process kill).
wait_for_adb_stable() {
    local deadline=$((SECONDS + 120))
    local healthy_count=0
    local state boot_completed dev_bootcomplete bootanim

    while [ "$SECONDS" -lt "$deadline" ]; do
        if ! kill -0 "$emulator_pid" 2>/dev/null; then
            echo "Android TV emulator process exited while waiting for adb to stabilize." >&2
            exit 1
        fi

        state="$(adb -s "$serial" get-state 2>/dev/null || echo unknown)"
        boot_completed="$(adb -s "$serial" shell getprop sys.boot_completed 2>/dev/null || echo unknown)"
        dev_bootcomplete="$(adb -s "$serial" shell getprop dev.bootcomplete 2>/dev/null || echo unknown)"
        bootanim="$(adb -s "$serial" shell getprop init.svc.bootanim 2>/dev/null || echo unknown)"

        if [ "$state" = "device" ] && [ "$boot_completed" = "1" ] && [ "$dev_bootcomplete" = "1" ] && [ "$bootanim" = "stopped" ]; then
            healthy_count=$((healthy_count + 1))
            if [ "$healthy_count" -ge 5 ]; then
                echo "adb connection stable."
                return 0
            fi
        else
            if [ "$healthy_count" -gt 0 ]; then
                echo "adb state dropped (state=$state, boot_completed=$boot_completed, dev.bootcomplete=$dev_bootcomplete, bootanim=$bootanim); restarting stability window."
            fi
            healthy_count=0
        fi

        sleep 2
    done

    echo "adb connection did not stabilize within 120 seconds." >&2
    exit 1
}

mkdir -p "$evidence_dir" "$ANDROID_AVD_HOME"

sdkmanager --install emulator "$system_image"
printf 'no\n' | avdmanager create avd \
    --force \
    --name "$avd_name" \
    --package "$system_image" \
    --device tv_1080p
{
    printf 'hw.cpu.ncore=2\n'
    printf 'hw.ramSize=2048\n'              # profile default is below the 1024 MB floor; Compose+swiftshader on 1 GB is tight
    printf 'disk.dataPartition.size=4096M\n' # default 7372.8 MB caused "Not enough space"; smoke run writes ~1 GB
} >> "$ANDROID_AVD_HOME/$avd_name.avd/config.ini"

adb start-server

echo "Disk space before emulator start:"
df -h /

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
echo "Started Android TV emulator (pid=$emulator_pid)."

booted=false
for _ in {1..300}; do
    if ! kill -0 "$emulator_pid" 2>/dev/null; then
        echo "Android TV emulator exited before completing boot." >&2
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
    exit 1
fi

echo "Android TV emulator reported sys.boot_completed at $(date -u +%FT%TZ)."

wait_for_adb_stable

"$root/tools/ci/run-android-tv-emulator-smoke.sh"
