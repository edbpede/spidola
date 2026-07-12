#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Spidola contributors
# SPDX-License-Identifier: AGPL-3.0-or-later
#
# Guard: secrets must never reach the log stream (TECH_SPEC §4.8, §12). Fails if any
# tracing/log macro formats an exposed secret value (`.expose(...)`) — the one way to get a
# raw secret out of a `Secret`. Multi-line macro calls are additionally covered by the
# secret types' redacted `Debug`; this grep catches the common single-line leak.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
pattern='\b(trace|debug|info|warn|error)!\s*\(.*\.expose\('

if grep -rnE "$pattern" "$root/crates" --include='*.rs'; then
  echo "error: a log macro formats an exposed secret value (matches above) — secrets must" >&2
  echo "       never reach the log stream (TECH_SPEC §4.8, §12)." >&2
  exit 1
fi

echo "ok: no exposed secrets formatted in log macros"
