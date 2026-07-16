#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Spidola contributors
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Validate the repository translation contribution path without service credentials."""

from __future__ import annotations

import argparse
import json
import sys
import xml.etree.ElementTree as ET
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]


def fail(message: str) -> None:
    print(f"translation validation: {message}", file=sys.stderr)
    raise SystemExit(1)


def translatable_android_keys(path: Path) -> set[str]:
    try:
        root = ET.parse(path).getroot()
    except (OSError, ET.ParseError) as error:
        fail(f"cannot parse {path}: {error}")
    keys = set()
    for element in root:
        name = element.attrib.get("name")
        if name and element.attrib.get("translatable", "true") != "false":
            keys.add(f"{element.tag}:{name}")
    return keys


def localization_is_complete(value: object) -> bool:
    if not isinstance(value, dict):
        return False
    unit = value.get("stringUnit")
    if isinstance(unit, dict):
        return unit.get("state") == "translated" and bool(unit.get("value"))
    variations = value.get("variations")
    if isinstance(variations, dict):
        children = [
            item
            for variation in variations.values()
            if isinstance(variation, dict)
            for item in variation.values()
        ]
        return bool(children) and all(localization_is_complete(item) for item in children)
    return False


def swift_catalogs(config_text: str) -> tuple[list[Path], set[str]]:
    paths = sorted(ROOT.glob("apps/tvos/Packages/*/Sources/*/Resources/Localizable.xcstrings"))
    locales: set[str] = set()
    catalog_data: list[tuple[Path, dict[str, object]]] = []
    for path in paths:
        if f"/{path.relative_to(ROOT).as_posix()}" not in config_text:
            fail(f"Crowdin config does not include {path.relative_to(ROOT)}")
        try:
            data = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as error:
            fail(f"cannot parse {path}: {error}")
        if data.get("sourceLanguage") != "en":
            fail(f"{path} must use English as sourceLanguage")
        strings = data.get("strings", {})
        if not isinstance(strings, dict):
            fail(f"{path} has no strings object")
        for entry in strings.values():
            if isinstance(entry, dict):
                localizations = entry.get("localizations", {})
                if isinstance(localizations, dict):
                    locales.update(localizations.keys())
        catalog_data.append((path, data))

    locales.discard("en")
    for locale in locales:
        for path, data in catalog_data:
            strings = data["strings"]
            for key, entry in strings.items():
                if not isinstance(entry, dict) or entry.get("shouldTranslate") is False:
                    continue
                localizations = entry.get("localizations", {})
                if not localization_is_complete(localizations.get(locale)):
                    fail(f"{path}: {locale} is incomplete at key {key!r}")
    return paths, locales


def android_catalogs(config_text: str) -> tuple[list[Path], set[str]]:
    bases = sorted(ROOT.glob("apps/androidtv/**/src/main/res/values/strings.xml"))
    locale_keys: dict[str, list[tuple[Path, set[str]]]] = {}
    for base in bases:
        if f"/{base.relative_to(ROOT).as_posix()}" not in config_text:
            fail(f"Crowdin config does not include {base.relative_to(ROOT)}")
        base_keys = translatable_android_keys(base)
        for directory in base.parent.parent.glob("values-*"):
            translated = directory / "strings.xml"
            if translated.is_file():
                locale = directory.name.removeprefix("values-").replace("-r", "-")
                locale_keys.setdefault(locale, []).append((translated, base_keys))

    for locale, translations in locale_keys.items():
        if len(translations) != len(bases):
            fail(f"Android locale {locale} exists in {len(translations)}/{len(bases)} catalogs")
        for translated, base_keys in translations:
            translated_keys = translatable_android_keys(translated)
            missing = sorted(base_keys - translated_keys)
            extra = sorted(translated_keys - base_keys)
            if missing or extra:
                fail(f"{translated}: missing={missing}, extra={extra}")
    return bases, set(locale_keys)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--require-locale",
        action="store_true",
        help="also require at least one complete non-English locale in both shells",
    )
    args = parser.parse_args()
    config_path = ROOT / "crowdin.yml"
    try:
        config_text = config_path.read_text(encoding="utf-8")
    except OSError as error:
        fail(f"cannot read {config_path}: {error}")

    swift_paths, swift_locales = swift_catalogs(config_text)
    android_paths, android_locales = android_catalogs(config_text)
    if swift_locales != android_locales:
        fail(f"shell locale sets differ (tvOS={sorted(swift_locales)}, Android={sorted(android_locales)})")
    if args.require_locale and not swift_locales:
        fail("no complete non-English locale exists yet")
    print(
        "translation validation: "
        f"{len(swift_paths)} tvOS + {len(android_paths)} Android catalogs; "
        f"complete shared locales={sorted(swift_locales)}"
    )


if __name__ == "__main__":
    main()
