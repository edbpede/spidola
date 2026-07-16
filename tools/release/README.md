<!--
SPDX-FileCopyrightText: 2026 Spidola contributors
SPDX-License-Identifier: AGPL-3.0-or-later
-->

# Release tooling

Generate a preview changelog from Conventional Commits:

```sh
python3 tools/release/generate-changelog.py --version Unreleased
```

Build the direct Android artifact after producing the pinned libmpv closure:

```sh
tools/build-libmpv-android/build.sh
tools/release/build-android-direct.sh --version 1.0.0 --version-code 1000000
```

That command is intentionally unsigned unless all four variables shown in
`apps/androidtv/signing.env.example` are present. Publication uses `--require-signing`, verifies
the APK signature, and refuses a package missing any of the three supported ABIs or native
libraries. Output lands under ignored `dist/android/` with a `SHA256SUMS` manifest.
