#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Spidola contributors
# SPDX-License-Identifier: AGPL-3.0-or-later
#
# Guard: the tvOS media stack must stay on the pinned **LGPL** MPVKit build (TECH_SPEC §12,
# PRD §10). Two things can silently break that, and this checks both:
#
#   1. Linking the GPL product. `MPVKit-GPL` is one character away from `MPVKit` in a manifest,
#      and nothing in a build would complain — the app would just become undistributable.
#   2. Drifting off the pin. A resolve that moves to a version we never checksummed would ship
#      binaries nobody reviewed, which is what the checksums exist to prevent.
#
# Runs in CI (.github/workflows/apple.yml) and offline: it reads the committed manifest, lock,
# and Package.resolved, and never touches the network.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
lock="$root/tools/build-mpvkit/mpvkit.lock"
manifest="$root/apps/tvos/Packages/PlayerMPV/Package.swift"
resolved="$root/apps/tvos/Packages/PlayerMPV/Package.resolved"

fail() {
  echo "error: $1" >&2
  exit 1
}

[ -f "$lock" ] || fail "missing $lock"
[ -f "$manifest" ] || fail "missing $manifest"

pinned_version="$(awk '$1 == "version" { print $2; exit }' "$lock")"
[ -n "$pinned_version" ] || fail "no 'version' line in $lock"

# 1. The GPL product must not be linked anywhere in our own sources.
#
# What is forbidden is *linking* it, and linking happens in code — so the scan narrows to code, in
# three steps, each for its own reason:
#
#   - Vendored checkouts and build outputs are skipped. Upstream legitimately vends both products,
#     so scanning its tree would fail on someone else's code, and build directories churn while
#     other builds run, which makes the guard both slow and flaky (grep racing a compiler's temp
#     files). This script is skipped because the pattern below *is* the string.
#   - Markdown is skipped, and comment lines are dropped below. This directory's README and the
#     manifest both name the GPL product in prose precisely in order to forbid it; a check that
#     fired on its own documentation would teach people to delete the documentation.
hits="$(
  grep -rn --binary-files=without-match "MPVKit-GPL" \
    "$root/apps" "$root/tools" "$root/.github" \
    --exclude-dir=.build \
    --exclude-dir=build \
    --exclude-dir=checkouts \
    --exclude-dir=.git \
    --exclude="*.md" \
    --exclude="verify-mpvkit-pin.sh" || true
)"
code_hits="$(
  printf '%s' "$hits" | awk '
    NF == 0 { next }
    {
      line = $0
      sub(/^[^:]*:[0-9]+:/, "", line)   # strip grep'"'"'s file:line: prefix
      sub(/^[ \t]+/, "", line)          # then leading indentation
      if (line !~ /^(\/\/|#|\*|<!--)/) print
    }'
)"
if [ -n "$code_hits" ]; then
  echo "$code_hits" >&2
  fail "the GPL MPVKit product is linked above — only the LGPL 'MPVKit' product may be used
       (TECH_SPEC §12, PRD §10)."
fi

# 2. The manifest must pin the exact version the lock records, via `exact:`.
#
# The `exact:` spelling is checked, not just the number: `from: "0.41.0"` would also contain the
# string but would let a resolve walk forward to an unchecksummed release.
if ! grep -q "MPVKit.git\", exact: \"$pinned_version\"" "$manifest"; then
  fail "$manifest does not pin MPVKit at exact: \"$pinned_version\" (the version in $lock).
       A range would let a resolve move off the checksummed artifacts."
fi

# 3. The manifest must link the LGPL product by name.
if ! grep -q '\.product(name: "MPVKit", package: "MPVKit")' "$manifest"; then
  fail "$manifest does not link the LGPL 'MPVKit' product."
fi

# 4. Package.resolved, when present, must agree with the pin.
#
# Absent is not a failure: a fresh checkout has not resolved yet, and CI resolves before it builds.
# Present and disagreeing is a failure — that is the drift this exists to catch.
if [ -f "$resolved" ]; then
  resolved_version="$(
    python3 -c '
import json, sys
with open(sys.argv[1]) as handle:
    pins = json.load(handle).get("pins", [])
for pin in pins:
    if pin.get("identity") == "mpvkit":
        print(pin.get("state", {}).get("version", ""))
        break
' "$resolved"
  )"
  [ -n "$resolved_version" ] || fail "$resolved has no pin for mpvkit."
  [ "$resolved_version" = "$pinned_version" ] || fail \
    "Package.resolved has MPVKit $resolved_version but the pin is $pinned_version.
       Re-resolve, or update $lock deliberately (see tools/build-mpvkit/README.md)."
fi

# 5. No GPL artifact may appear in the lock's closure.
#
# Checked before the count so a smuggled GPL entry is reported as what it is. The entry pattern
# below accepts hyphens precisely so `Libavcodec-GPL` parses as a well-formed entry and reaches
# this check, rather than being silently miscounted and blamed on the tally.
if grep -qE '^(Libsmbclient|[A-Za-z0-9_]+-GPL)[[:space:]]' "$lock"; then
  fail "$lock lists a GPL-only artifact. The LGPL closure must not contain one
       (TECH_SPEC §12, PRD §10)."
fi

# 6. The lock must be internally consistent: a real sha256 per artifact, and the count it claims.
declared="$(awk '$1 == "artifacts" { print $2; exit }' "$lock")"
[ -n "$declared" ] || fail "no 'artifacts' count in $lock"

actual="$(grep -cE '^[A-Za-z_][A-Za-z0-9_-]*[[:space:]]+[0-9a-f]{64}[[:space:]]+https://' "$lock")"
[ "$declared" = "$actual" ] || fail \
  "$lock claims $declared artifacts but $actual well-formed entries were found.
       A malformed checksum (not 64 hex chars) also lands here."

echo "ok: MPVKit pinned at $pinned_version (LGPL product), $actual artifacts checksummed"
