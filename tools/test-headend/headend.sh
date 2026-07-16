#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Spidola contributors
# SPDX-License-Identifier: AGPL-3.0-or-later

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
assets="${SPIDOLA_HEADEND_ASSETS:-${repo_root}/target/test-headend-assets}"
runtime_dir="${repo_root}/target/test-headend-runtime"
pid_file="${runtime_dir}/headend.pid"
log_file="${runtime_dir}/headend.log"
bind="${SPIDOLA_HEADEND_BIND:-0.0.0.0:8090}"
public_base="${SPIDOLA_HEADEND_PUBLIC_BASE:-http://127.0.0.1:8090}"
health_url="${SPIDOLA_HEADEND_HEALTH_URL:-http://127.0.0.1:${bind##*:}/manifest.json}"
binary="${repo_root}/target/debug/spidola-test-headend"

usage() {
    printf '%s\n' \
        'Usage: tools/test-headend/headend.sh generate|run|start|stop|status' \
        '' \
        'Environment overrides:' \
        '  SPIDOLA_HEADEND_ASSETS          generated asset directory' \
        '  SPIDOLA_HEADEND_BIND            listen address (default 0.0.0.0:8090)' \
        '  SPIDOLA_HEADEND_PUBLIC_BASE     URLs emitted in manifest.json' \
        '  SPIDOLA_HEADEND_HEALTH_URL      local readiness URL' \
        '  SPIDOLA_HEADEND_DURATION_SECONDS fixture duration (default 60)' \
        '  SPIDOLA_HEADEND_STALL_SECONDS   /timeout stall (default 300)' \
        '  SPIDOLA_HEADEND_DROP_SECONDS    /mid-stream-drop duration (default 20)'
}

build() {
    cargo build --manifest-path "${repo_root}/Cargo.toml" -p spidola-test-headend
}

run_foreground() {
    build
    exec "${binary}" \
        --bind "${bind}" \
        --assets "${assets}" \
        --public-base "${public_base}" \
        --stall-seconds "${SPIDOLA_HEADEND_STALL_SECONDS:-300}" \
        --drop-seconds "${SPIDOLA_HEADEND_DROP_SECONDS:-20}"
}

is_running() {
    [[ -f "${pid_file}" ]] && kill -0 "$(<"${pid_file}")" 2>/dev/null
}

start() {
    if is_running; then
        printf 'test headend is already running (pid %s)\n' "$(<"${pid_file}")"
        return
    fi
    build
    mkdir -p "${runtime_dir}"
    nohup "${binary}" \
        --bind "${bind}" \
        --assets "${assets}" \
        --public-base "${public_base}" \
        --stall-seconds "${SPIDOLA_HEADEND_STALL_SECONDS:-300}" \
        --drop-seconds "${SPIDOLA_HEADEND_DROP_SECONDS:-20}" \
        >"${log_file}" 2>&1 &
    printf '%s\n' "$!" > "${pid_file}"
    for _ in {1..50}; do
        if curl --fail --silent --max-time 1 "${health_url}" >/dev/null; then
            printf 'test headend started at %s (pid %s)\n' "${public_base}" "$(<"${pid_file}")"
            return
        fi
        if ! is_running; then
            printf 'test headend exited during startup; see %s\n' "${log_file}" >&2
            rm -f "${pid_file}"
            exit 1
        fi
        sleep 0.1
    done
    printf 'test headend did not become ready; see %s\n' "${log_file}" >&2
    stop
    exit 1
}

stop() {
    if ! is_running; then
        rm -f "${pid_file}"
        printf '%s\n' 'test headend is not running'
        return
    fi
    local pid
    pid="$(<"${pid_file}")"
    kill "${pid}"
    for _ in {1..50}; do
        if ! kill -0 "${pid}" 2>/dev/null; then
            rm -f "${pid_file}"
            printf '%s\n' 'test headend stopped'
            return
        fi
        sleep 0.1
    done
    printf 'test headend pid %s did not stop after SIGTERM\n' "${pid}" >&2
    exit 1
}

status() {
    if is_running; then
        printf 'test headend is running at %s (pid %s)\n' "${public_base}" "$(<"${pid_file}")"
    else
        printf '%s\n' 'test headend is not running'
        return 1
    fi
}

case "${1:-}" in
    generate)
        "${repo_root}/tools/test-headend/generate-assets.sh" "${assets}"
        ;;
    run)
        run_foreground
        ;;
    start)
        start
        ;;
    stop)
        stop
        ;;
    status)
        status
        ;;
    *)
        usage >&2
        exit 2
        ;;
esac
