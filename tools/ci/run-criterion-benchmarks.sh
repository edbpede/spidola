#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Spidola contributors
# SPDX-License-Identifier: AGPL-3.0-or-later

set -euo pipefail

if [[ $# -lt 2 || $# -gt 3 ]]; then
  echo "usage: $0 REPOSITORY TARGET_DIR [CONFIG]" >&2
  exit 2
fi

repository="$(cd "$1" && pwd)"
mkdir -p "$2"
target_dir="$(cd "$2" && pwd)"
config="${3:-$(cd "$(dirname "$0")" && pwd)/criterion-benchmarks.txt}"
if [[ "$config" != /* ]]; then
  config="$(pwd)/$config"
fi

if [[ ! -f "$config" ]]; then
  echo "criterion runner: config not found: $config" >&2
  exit 2
fi

manifest="$repository/Cargo.toml"
if [[ ! -f "$manifest" ]]; then
  echo "criterion runner: Cargo workspace not found: $repository" >&2
  exit 2
fi

# rust-cache may restore old Criterion reports. Keep compiled artifacts, but ensure this
# invocation records only benchmarks produced from the requested revision.
rm -rf "$target_dir/criterion"

while read -r package bench source metric extra; do
  if [[ -z "$package" || "$package" == \#* ]]; then
    continue
  fi
  if [[ -z "$bench" || -z "$source" || -z "$metric" || -n "$extra" ]]; then
    echo "criterion runner: malformed config row: $package $bench $source $metric $extra" >&2
    exit 2
  fi
  if [[ ! -f "$repository/$source" ]]; then
    echo "criterion runner: skipping new benchmark absent from this revision: $metric"
    continue
  fi

  echo "criterion runner: $package --bench $bench ($metric)"
  (
    cd "$repository"
    CARGO_TARGET_DIR="$target_dir" cargo bench --locked -p "$package" --bench "$bench" -- --noplot
  )
  mkdir -p "$target_dir/criterion"
  printf '%s\n' "$metric" >>"$target_dir/criterion/spidola-ran-benchmarks.txt"
done <"$config"
