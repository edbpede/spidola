#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Spidola contributors
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Validate Criterion output and reject statistically clear performance regressions."""

from __future__ import annotations

import argparse
import json
import math
import sys
from dataclasses import dataclass
from pathlib import Path


DEFAULT_CONFIG = Path(__file__).with_name("criterion-benchmarks.txt")
RUN_MANIFEST = "spidola-ran-benchmarks.txt"


class GateInputError(ValueError):
    """Raised when gate configuration or Criterion output is invalid."""


@dataclass(frozen=True)
class Benchmark:
    package: str
    target: str
    source: str
    metric: str


@dataclass(frozen=True)
class Estimate:
    point: float
    lower: float
    upper: float


@dataclass(frozen=True)
class Comparison:
    metric: str
    point_change_percent: float | None
    regressed: bool
    introduced: bool


def load_benchmarks(path: Path) -> list[Benchmark]:
    benchmarks: list[Benchmark] = []
    seen_metrics: set[str] = set()
    try:
        lines = path.read_text(encoding="utf-8").splitlines()
    except OSError as error:
        raise GateInputError(f"cannot read config {path}: {error}") from error

    for line_number, raw_line in enumerate(lines, start=1):
        line = raw_line.strip()
        if not line or line.startswith("#"):
            continue
        fields = line.split()
        if len(fields) != 4:
            raise GateInputError(f"{path}:{line_number}: expected four fields")
        benchmark = Benchmark(*fields)
        metric_path = Path(benchmark.metric)
        if metric_path.is_absolute() or ".." in metric_path.parts:
            raise GateInputError(f"{path}:{line_number}: unsafe metric path")
        if benchmark.metric in seen_metrics:
            raise GateInputError(f"{path}:{line_number}: duplicate metric {benchmark.metric}")
        seen_metrics.add(benchmark.metric)
        benchmarks.append(benchmark)

    if not benchmarks:
        raise GateInputError(f"{path}: no benchmarks configured")
    return benchmarks


def load_estimate(root: Path, metric: str) -> Estimate | None:
    path = root / metric / "new" / "estimates.json"
    if not path.is_file():
        return None
    try:
        document = json.loads(path.read_text(encoding="utf-8"))
        mean = document["mean"]
        interval = mean["confidence_interval"]
        estimate = Estimate(
            point=float(mean["point_estimate"]),
            lower=float(interval["lower_bound"]),
            upper=float(interval["upper_bound"]),
        )
    except (OSError, json.JSONDecodeError, KeyError, TypeError, ValueError) as error:
        raise GateInputError(f"invalid Criterion estimate {path}: {error}") from error

    values = (estimate.point, estimate.lower, estimate.upper)
    if not all(math.isfinite(value) and value > 0 for value in values):
        raise GateInputError(f"invalid non-positive or non-finite estimate in {path}")
    if not estimate.lower <= estimate.point <= estimate.upper:
        raise GateInputError(f"invalid confidence interval in {path}")
    return estimate


def load_run_manifest(root: Path) -> set[str]:
    path = root / RUN_MANIFEST
    if not path.is_file():
        return set()
    try:
        return {line.strip() for line in path.read_text(encoding="utf-8").splitlines() if line.strip()}
    except OSError as error:
        raise GateInputError(f"cannot read run manifest {path}: {error}") from error


def ensure_baseline_metrics_retained(
    candidate: list[Benchmark], baseline: list[Benchmark]
) -> None:
    candidate_metrics = {benchmark.metric for benchmark in candidate}
    removed = sorted(
        benchmark.metric for benchmark in baseline if benchmark.metric not in candidate_metrics
    )
    if removed:
        raise GateInputError(
            "candidate config removed or renamed baseline metric(s): " + ", ".join(removed)
        )


def compare(
    benchmarks: list[Benchmark],
    candidate_root: Path,
    baseline_root: Path | None,
    max_regression_percent: float,
) -> list[Comparison]:
    if not math.isfinite(max_regression_percent) or max_regression_percent < 0:
        raise GateInputError("maximum regression percentage must be finite and non-negative")
    if not candidate_root.is_dir():
        raise GateInputError(f"candidate Criterion root not found: {candidate_root}")
    if baseline_root is not None and not baseline_root.is_dir():
        raise GateInputError(f"baseline Criterion root not found: {baseline_root}")

    baseline_ran = load_run_manifest(baseline_root) if baseline_root is not None else set()
    comparisons: list[Comparison] = []
    factor = 1 + max_regression_percent / 100
    for benchmark in benchmarks:
        candidate = load_estimate(candidate_root, benchmark.metric)
        if candidate is None:
            raise GateInputError(f"candidate estimate missing for {benchmark.metric}")

        baseline = (
            load_estimate(baseline_root, benchmark.metric)
            if baseline_root is not None
            else None
        )
        if baseline is None:
            if benchmark.metric in baseline_ran:
                raise GateInputError(f"baseline ran but estimate is missing for {benchmark.metric}")
            comparisons.append(
                Comparison(benchmark.metric, None, regressed=False, introduced=True)
            )
            continue

        point_change = (candidate.point / baseline.point - 1) * 100
        # Fail only when the candidate's entire mean confidence interval is beyond the
        # permitted regression from the baseline interval. This avoids runner-noise failures.
        regressed = candidate.lower > baseline.upper * factor
        comparisons.append(
            Comparison(benchmark.metric, point_change, regressed=regressed, introduced=False)
        )
    return comparisons


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--config", type=Path, default=DEFAULT_CONFIG)
    parser.add_argument("--candidate", type=Path, required=True)
    parser.add_argument("--baseline", type=Path)
    parser.add_argument("--baseline-config", type=Path)
    parser.add_argument("--max-regression-percent", type=float, default=20.0)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        benchmarks = load_benchmarks(args.config)
        if (args.baseline is None) != (args.baseline_config is None):
            raise GateInputError("--baseline and --baseline-config must be provided together")
        if args.baseline_config is not None:
            ensure_baseline_metrics_retained(
                benchmarks, load_benchmarks(args.baseline_config)
            )
        comparisons = compare(
            benchmarks,
            args.candidate,
            args.baseline,
            args.max_regression_percent,
        )
    except GateInputError as error:
        print(f"criterion gate: {error}", file=sys.stderr)
        return 2

    regressions = 0
    for comparison in comparisons:
        if comparison.introduced:
            print(f"criterion gate: NEW {comparison.metric} (candidate estimate validated)")
            continue
        status = "REGRESSION" if comparison.regressed else "PASS"
        print(
            f"criterion gate: {status} {comparison.metric} "
            f"mean change={comparison.point_change_percent:+.2f}%"
        )
        regressions += int(comparison.regressed)

    if regressions:
        print(
            f"criterion gate: {regressions} benchmark(s) exceeded the "
            f"{args.max_regression_percent:g}% confidence-interval ceiling",
            file=sys.stderr,
        )
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
