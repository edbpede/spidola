#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Spidola contributors
# SPDX-License-Identifier: AGPL-3.0-or-later
#
# Guard: the bundled media stack stays pinned and stays LGPL (PRD §10, TECH_SPEC §12).
#
# Fails if:
#   1. sources.lock is malformed, or names a source twice.
#   2. A fetched source's digest has drifted from sources.lock.
#   3. Any build config asks for GPL or nonfree code.
#   4. FFmpeg/mpv do not positively assert their LGPL flags.
#   5. A configured build tree resolved to a non-LGPL licence.
#
# Runs offline and in seconds by default, so it belongs in the Android CI lane next to the
# lint tasks. `--fetch` additionally downloads every pinned source and re-derives its
# checksum, which is the full drift check — slower, and network-bound.
#
# Usage: verify-pins.sh [--fetch]
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly LOCK="$here/sources.lock"

failures=0
note_failure() {
  echo "error: $*" >&2
  failures=$((failures + 1))
}

sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d' ' -f1
  else
    shasum -a 256 "$1" | cut -d' ' -f1
  fi
}

lock_records() {
  grep -vE '^\s*(#|$)' "$LOCK"
}

# The files that actually decide what gets fetched and how it is configured. Deliberately
# NOT the README: the README's whole job is to explain the GPL flags we refuse, so scanning
# it for those flags would make documenting the policy impossible.
config_files() {
  printf '%s\n' \
    "$LOCK" \
    "$here/build.sh" \
    "$here/fetch.sh" \
    "$here/toolchain.sh"
  find "$here/components" -name '*.sh' -type f | sort
}

# ── 1. sources.lock is well-formed ──────────────────────────────────────────
check_lockfile() {
  local name kind version digest url count
  count=0
  while read -r name kind version digest url; do
    count=$((count + 1))
    case "$kind" in
      tarball)
        [ "${#digest}" -eq 64 ] || note_failure "sources.lock: $name has a ${#digest}-char digest; a sha256 is 64"
        ;;
      git | git-submodule)
        [ "${#digest}" -eq 40 ] || note_failure "sources.lock: $name has a ${#digest}-char digest; a git commit SHA is 40"
        ;;
      *)
        note_failure "sources.lock: unknown kind '$kind' for $name"
        ;;
    esac
    [ -n "$url" ] || note_failure "sources.lock: $name has no URL"
  done < <(lock_records)

  [ "$count" -gt 0 ] || note_failure "sources.lock: no records parsed"

  local dupes
  dupes="$(lock_records | awk '{print $1}' | sort | uniq -d)"
  [ -z "$dupes" ] || note_failure "sources.lock: duplicate entries: $dupes"

  echo "ok: sources.lock parses ($count pinned sources)"
}

# ── 2. Fetched sources still match their pins ───────────────────────────────
check_digests() {
  local checked=0 name kind version digest url
  while read -r name kind version digest url; do
    case "$kind" in
      tarball)
        local archive="$here/downloads/${url##*/}"
        [ -f "$archive" ] || continue
        local have
        have="$(sha256_of "$archive")"
        if [ "$have" != "$digest" ]; then
          note_failure "$name: sha256 drift
    lockfile $digest
    actual   $have
    archive  $archive"
        fi
        checked=$((checked + 1))
        ;;
      git | git-submodule)
        local repo="$here/src/$name"
        [ -d "$repo/.git" ] || continue
        local have
        have="$(git -C "$repo" rev-parse HEAD)"
        if [ "$have" != "$digest" ]; then
          note_failure "$name: commit drift
    lockfile $digest
    checkout $have"
        fi
        checked=$((checked + 1))
        ;;
    esac
  done < <(lock_records)

  if [ "$checked" -eq 0 ]; then
    echo "note: no sources fetched yet — digests not re-derived (run with --fetch for the full check)"
  else
    echo "ok: $checked fetched source(s) match sources.lock"
  fi
}

# ── 3. No build config asks for GPL or nonfree code ─────────────────────────
check_no_gpl_flags() {
  # -F: these are literal flags, not patterns. Note --disable-gpl does not contain the
  # string --enable-gpl, so the explicit negative flags in ffmpeg.sh do not trip this.
  local forbidden=(--enable-gpl --enable-nonfree -Dgpl=true)
  local flag hits found=0
  for flag in "${forbidden[@]}"; do
    hits="$(config_files | xargs grep -nF -- "$flag" 2>/dev/null || true)"
    if [ -n "$hits" ]; then
      found=1
      note_failure "build config requests '$flag' — the bundled media stack must be LGPL
  (PRD §10, TECH_SPEC §12). Found at:
$hits"
    fi
  done
  [ "$found" -eq 0 ] && echo "ok: no GPL/nonfree opt-in anywhere in the build config"
  return 0
}

# ── 4. FFmpeg and mpv positively assert LGPL ────────────────────────────────
# The absence of a GPL flag is not the same claim as the presence of an LGPL one: a
# refactor that dropped ffmpeg.sh's flags entirely would pass check 3 while quietly
# depending on an upstream default. Require the intent to be written down.
check_lgpl_flags_present() {
  local ffmpeg="$here/components/ffmpeg.sh"
  local mpv="$here/components/mpv.sh"

  grep -qF -- '--disable-gpl' "$ffmpeg" || note_failure "components/ffmpeg.sh no longer passes --disable-gpl"
  grep -qF -- '--disable-nonfree' "$ffmpeg" || note_failure "components/ffmpeg.sh no longer passes --disable-nonfree"
  grep -qF -- '--disable-postproc' "$ffmpeg" || note_failure "components/ffmpeg.sh no longer passes --disable-postproc (libpostproc is GPL-only)"
  grep -qF -- '-Dgpl=false' "$mpv" || note_failure "components/mpv.sh no longer passes -Dgpl=false"

  # Each component script asserts its own generated licence at build time; if that assertion
  # is deleted, the build stops proving anything and this gate is all that is left.
  grep -qF 'FFMPEG_LICENSE' "$ffmpeg" || note_failure "components/ffmpeg.sh dropped its FFMPEG_LICENSE assertion"
  grep -qF 'HAVE_GPL 0' "$mpv" || note_failure "components/mpv.sh dropped its HAVE_GPL assertion"

  echo "ok: ffmpeg (--disable-gpl --disable-nonfree) and mpv (-Dgpl=false) assert LGPL"
}

# ── 5. Any configured build tree actually resolved to LGPL ──────────────────
# The strongest check available, because it reads what the build systems concluded rather
# than what we asked for. Only possible once a build has been configured.
check_configured_licenses() {
  local checked=0 config
  while IFS= read -r config; do
    if grep -qF 'FFMPEG_LICENSE' "$config" 2>/dev/null; then
      # The invariant is "LGPL, of whichever version", not one exact string. v2.1 and v3 are
      # both LGPL and both AGPL-compatible, and which one you land on is decided by the TLS
      # provider (see components/ffmpeg.sh). What must never appear here is a bare "GPL
      # version ..." or "nonfree" — hence anchoring on the LGPL prefix rather than listing
      # the two acceptable strings and having to edit this on a TLS change.
      if ! grep -qE '^#define FFMPEG_LICENSE "LGPL version (2\.1|3) or later"$' "$config"; then
        note_failure "configured ffmpeg is not LGPL: $config
    says: $(grep -F 'FFMPEG_LICENSE' "$config")"
      fi
      checked=$((checked + 1))
    fi
  done < <(find "$here/build" -name config.h -path '*ffmpeg*' -type f 2>/dev/null || true)

  while IFS= read -r config; do
    if grep -qE '^#define HAVE_GPL' "$config" 2>/dev/null; then
      if ! grep -qE '^#define HAVE_GPL 0$' "$config"; then
        note_failure "configured mpv is not LGPL: $config
    says: $(grep -E '^#define HAVE_GPL' "$config")"
      fi
      checked=$((checked + 1))
    fi
  done < <(find "$here/build" -name config.h -path '*mpv*' -type f 2>/dev/null || true)

  if [ "$checked" -eq 0 ]; then
    echo "note: no configured build tree — generated licences not checked (run build.sh first)"
  else
    echo "ok: $checked configured build tree(s) resolved to LGPL"
  fi
}

main() {
  if [ "${1:-}" = "--fetch" ]; then
    echo "==> fetching pinned sources (full drift check)"
    "$here/fetch.sh"
  fi

  check_lockfile
  check_digests
  check_no_gpl_flags
  check_lgpl_flags_present
  check_configured_licenses

  if [ "$failures" -gt 0 ]; then
    echo >&2
    echo "verify-pins: $failures check(s) failed" >&2
    exit 1
  fi
  echo
  echo "verify-pins: all checks passed"
}

main "$@"
