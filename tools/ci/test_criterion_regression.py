#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Spidola contributors
# SPDX-License-Identifier: AGPL-3.0-or-later

from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path

from criterion_regression import (
    Benchmark,
    GateInputError,
    compare,
    ensure_baseline_metrics_retained,
    load_benchmarks,
)


BENCHMARK = Benchmark("core-parse", "m3u", "benches/m3u.rs", "parse/50k")


def write_estimate(root: Path, point: float, lower: float, upper: float) -> None:
    path = root / BENCHMARK.metric / "new" / "estimates.json"
    path.parent.mkdir(parents=True)
    path.write_text(
        json.dumps(
            {
                "mean": {
                    "confidence_interval": {
                        "lower_bound": lower,
                        "upper_bound": upper,
                    },
                    "point_estimate": point,
                    "standard_error": 1.0,
                }
            }
        ),
        encoding="utf-8",
    )


class CriterionRegressionTests(unittest.TestCase):
    def test_clear_regression_fails(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            baseline = root / "baseline"
            candidate = root / "candidate"
            write_estimate(baseline, 100.0, 99.0, 101.0)
            write_estimate(candidate, 130.0, 128.0, 132.0)

            result = compare([BENCHMARK], candidate, baseline, 20.0)

            self.assertTrue(result[0].regressed)

    def test_overlapping_confidence_interval_does_not_fail(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            baseline = root / "baseline"
            candidate = root / "candidate"
            write_estimate(baseline, 100.0, 98.0, 102.0)
            write_estimate(candidate, 123.0, 121.0, 125.0)

            result = compare([BENCHMARK], candidate, baseline, 20.0)

            self.assertFalse(result[0].regressed)

    def test_new_benchmark_validates_without_baseline(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            baseline = root / "baseline"
            candidate = root / "candidate"
            baseline.mkdir()
            write_estimate(candidate, 100.0, 99.0, 101.0)

            result = compare([BENCHMARK], candidate, baseline, 20.0)

            self.assertTrue(result[0].introduced)
            self.assertFalse(result[0].regressed)

    def test_missing_candidate_is_an_error(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            candidate = Path(directory) / "candidate"
            candidate.mkdir()
            with self.assertRaisesRegex(GateInputError, "candidate estimate missing"):
                compare([BENCHMARK], candidate, None, 20.0)

    def test_missing_baseline_root_is_an_error(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            candidate = root / "candidate"
            write_estimate(candidate, 100.0, 99.0, 101.0)

            with self.assertRaisesRegex(GateInputError, "baseline Criterion root not found"):
                compare([BENCHMARK], candidate, root / "missing", 20.0)

    def test_missing_estimate_after_baseline_run_is_an_error(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            baseline = root / "baseline"
            candidate = root / "candidate"
            write_estimate(candidate, 100.0, 99.0, 101.0)
            baseline.mkdir()
            (baseline / "spidola-ran-benchmarks.txt").write_text(
                f"{BENCHMARK.metric}\n", encoding="utf-8"
            )

            with self.assertRaisesRegex(GateInputError, "baseline ran"):
                compare([BENCHMARK], candidate, baseline, 20.0)

    def test_config_rejects_duplicate_metrics(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            config = Path(directory) / "benchmarks.txt"
            config.write_text(
                "pkg first source.rs group/name\n"
                "pkg second other.rs group/name\n",
                encoding="utf-8",
            )

            with self.assertRaisesRegex(GateInputError, "duplicate metric"):
                load_benchmarks(config)

    def test_candidate_cannot_remove_or_rename_baseline_metric(self) -> None:
        renamed = Benchmark("core-parse", "m3u", "benches/m3u.rs", "parse/renamed")

        with self.assertRaisesRegex(GateInputError, "removed or renamed.*parse/50k"):
            ensure_baseline_metrics_retained([renamed], [BENCHMARK])

    def test_candidate_can_add_a_metric(self) -> None:
        added = Benchmark("core-parse", "xmltv", "benches/xmltv.rs", "xmltv/50k")

        ensure_baseline_metrics_retained([BENCHMARK, added], [BENCHMARK])


if __name__ == "__main__":
    unittest.main()
