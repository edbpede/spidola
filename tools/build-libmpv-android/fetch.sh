#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Spidola contributors
# SPDX-License-Identifier: AGPL-3.0-or-later
#
# Fetches every source named in sources.lock into ./downloads, and unpacks it into ./src.
# Nothing is unpacked whose digest does not match the lockfile, so a compromised mirror or a
# retagged upstream fails here rather than shipping inside libmpv.so.
#
# Usage: fetch.sh [name ...]   (default: everything in sources.lock)
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./toolchain.sh
source "$here/toolchain.sh"

readonly LOCK="$here/sources.lock"
readonly DOWNLOADS="$here/downloads"
readonly SRC="$here/src"

sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d' ' -f1
  else
    shasum -a 256 "$1" | cut -d' ' -f1
  fi
}

# Emits the lockfile's records as "name kind version digest url", comments and blanks removed.
lock_records() {
  grep -vE '^\s*(#|$)' "$LOCK"
}

lock_field() { # name field-index
  lock_records | awk -v n="$1" -v f="$2" '$1 == n { print $f; exit }'
}

fetch_tarball() { # name version digest url
  local name="$1" version="$2" want="$3" url="$4"
  local archive="$DOWNLOADS/${url##*/}"

  if [ -f "$archive" ]; then
    local have
    have="$(sha256_of "$archive")"
    # A cached file with the wrong digest is more likely a truncated download than an
    # attack, but either way it must not be trusted or silently reused.
    [ "$have" = "$want" ] || { echo "  cached $name digest mismatch — refetching" >&2; rm -f "$archive"; }
  fi

  if [ ! -f "$archive" ]; then
    echo "  fetching $name $version"
    curl -fL --retry 3 --retry-delay 2 -o "$archive.part" "$url" || fail "download failed: $url"
    mv "$archive.part" "$archive"
  fi

  local have
  have="$(sha256_of "$archive")"
  [ "$have" = "$want" ] || fail "$name digest mismatch
  expected $want
  actual   $have
  from     $url
This is either upstream retagging a release, a corrupted mirror, or tampering. Do not
'fix' it by editing sources.lock until the cause is known."

  local dest="$SRC/$name"
  if [ ! -d "$dest" ]; then
    echo "  unpacking $name"
    mkdir -p "$dest"
    # --strip-components=1: every one of these archives wraps its tree in a single
    # versioned directory, and the component scripts should not have to know its name.
    tar -xf "$archive" -C "$dest" --strip-components=1
  fi
}

fetch_git() { # name version commit url
  local name="$1" version="$2" want="$3" url="$4"
  local dest="$SRC/$name"

  if [ ! -d "$dest/.git" ]; then
    echo "  cloning $name $version"
    rm -rf "$dest"
    mkdir -p "$dest"
    git init -q "$dest"
    git -C "$dest" remote add origin "$url"
  fi

  if [ "$(git -C "$dest" rev-parse HEAD 2>/dev/null || true)" != "$want" ]; then
    # --depth 1 against an explicit commit: no history downloaded, and no tag indirection
    # that upstream could later move.
    git -C "$dest" fetch -q --depth 1 origin "$want" || fail "$name: cannot fetch $want from $url"
    git -C "$dest" checkout -q --detach FETCH_HEAD
  fi

  local have
  have="$(git -C "$dest" rev-parse HEAD)"
  [ "$have" = "$want" ] || fail "$name commit mismatch: expected $want, found $have"
}

fetch_submodule() { # path version commit url
  local path="$1" want="$3" url="$4"
  local dest="$SRC/$path"

  if [ ! -d "$dest/.git" ]; then
    echo "  cloning submodule $path"
    rm -rf "$dest"
    mkdir -p "$dest"
    git init -q "$dest"
    git -C "$dest" remote add origin "$url"
  fi

  if [ "$(git -C "$dest" rev-parse HEAD 2>/dev/null || true)" != "$want" ]; then
    git -C "$dest" fetch -q --depth 1 origin "$want" || fail "$path: cannot fetch $want from $url"
    git -C "$dest" checkout -q --detach FETCH_HEAD
  fi

  local have
  have="$(git -C "$dest" rev-parse HEAD)"
  [ "$have" = "$want" ] || fail "$path commit mismatch: expected $want, found $have"
}

main() {
  mkdir -p "$DOWNLOADS" "$SRC"
  local wanted=("$@")

  local name kind version digest url
  while read -r name kind version digest url; do
    if [ ${#wanted[@]} -gt 0 ] && ! printf '%s\n' "${wanted[@]}" | grep -qx "$name"; then
      continue
    fi
    case "$kind" in
      tarball) fetch_tarball "$name" "$version" "$digest" "$url" ;;
      git) fetch_git "$name" "$version" "$digest" "$url" ;;
      git-submodule) fetch_submodule "$name" "$version" "$digest" "$url" ;;
      *) fail "sources.lock: unknown kind '$kind' for $name" ;;
    esac
  done < <(lock_records)

  echo "fetch: all sources match sources.lock"
}

main "$@"
