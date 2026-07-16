#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Spidola contributors
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Generate a deterministic Markdown changelog from Conventional Commits."""

from __future__ import annotations

import argparse
import re
import subprocess
from collections import defaultdict
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
CONVENTIONAL = re.compile(
    r"^(?P<type>[a-z][a-z0-9-]*)(?:\((?P<scope>[^)]+)\))?(?P<breaking>!)?: (?P<title>.+)$"
)
CATEGORIES = {
    "feat": "Features",
    "fix": "Fixes",
    "perf": "Performance",
    "refactor": "Refactoring",
    "docs": "Documentation",
    "test": "Tests",
    "build": "Build",
    "ci": "Continuous integration",
    "chore": "Maintenance",
}
CATEGORY_ORDER = [
    "Breaking changes",
    "Features",
    "Fixes",
    "Performance",
    "Refactoring",
    "Documentation",
    "Tests",
    "Build",
    "Continuous integration",
    "Maintenance",
    "Other",
]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--from-ref", help="exclusive lower bound; defaults to the latest tag")
    parser.add_argument("--to-ref", default="HEAD")
    parser.add_argument("--version", default="Unreleased")
    parser.add_argument("--output", type=Path, default=ROOT / "CHANGELOG.md")
    return parser.parse_args()


def git(*args: str, check: bool = True, strip: bool = True) -> str:
    result = subprocess.run(
        ["git", "-C", str(ROOT), *args],
        check=check,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
    )
    return result.stdout.strip() if strip else result.stdout


def latest_tag() -> str | None:
    tag = git("describe", "--tags", "--abbrev=0", check=False)
    return tag or None


def commits(revision_range: str) -> list[tuple[str, str, str]]:
    output = git(
        "log",
        "--no-merges",
        "--format=%H%x1f%s%x1f%b%x1e",
        revision_range,
        strip=False,
    )
    parsed = []
    for record in output.split("\x1e"):
        fields = record.strip("\n").split("\x1f", 2)
        if len(fields) == 3:
            parsed.append((fields[0], fields[1], fields[2]))
    return parsed


def render(version: str, entries: list[tuple[str, str, str]]) -> str:
    sections: dict[str, list[str]] = defaultdict(list)
    for commit_hash, subject, body in entries:
        match = CONVENTIONAL.match(subject)
        if match:
            commit_type = match.group("type")
            title = match.group("title")
            scope = match.group("scope")
            breaking = bool(match.group("breaking")) or "BREAKING CHANGE:" in body
            category = "Breaking changes" if breaking else CATEGORIES.get(commit_type, "Other")
            label = f"**{scope}:** {title}" if scope else title
        else:
            category = "Other"
            label = subject
        sections[category].append(f"- {label} (`{commit_hash[:7]}`)")

    lines = [
        "<!--",
        "SPDX-FileCopyrightText: 2026 Spidola contributors",
        "SPDX-License-" "Identifier: AGPL-3.0-or-later",
        "-->",
        "",
        "# Changelog",
        "",
        f"## {version}",
    ]
    for category in CATEGORY_ORDER:
        if sections[category]:
            lines.extend(["", f"### {category}", "", *sections[category]])
    if not entries:
        lines.extend(["", "No user-visible changes."])
    return "\n".join(lines) + "\n"


def main() -> None:
    args = parse_args()
    lower = args.from_ref or latest_tag()
    revision_range = f"{lower}..{args.to_ref}" if lower else args.to_ref
    output = render(args.version, commits(revision_range))
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(output, encoding="utf-8")
    print(f"changelog: wrote {args.output} from {revision_range}")


if __name__ == "__main__":
    main()
