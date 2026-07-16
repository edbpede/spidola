<!--
SPDX-FileCopyrightText: 2026 Spidola contributors
SPDX-License-Identifier: AGPL-3.0-or-later
-->

# Shell dependency license gates

The Android application applies Cash App Licensee 1.14.1 to the leaf `app` module. Its
variant task walks the complete external Gradle graph, fails on missing or disallowed license
metadata, and packages the resolved report at
`assets/licenses/android-dependencies.json` for the About surface:

```sh
apps/androidtv/gradlew --project-dir apps/androidtv :app:licenseeRelease
```

The SwiftPM graph is small but includes binary artifacts, so its gate is deliberately pinned.
`Package.resolved`, the reviewed coordinates/license in `swiftpm-policy.json`, the upstream
license text, and the committed About resource must all agree:

```sh
swift package --package-path apps/tvos/Packages/PlayerMPV resolve
python3 tools/licenses/check-swiftpm-licenses.py \
  --notice-output apps/tvos/App/Resources/ThirdPartyNotices.txt \
  --check-notice
```

To accept a dependency update, review the new package and its complete binary closure, update
the policy explicitly, then regenerate the notice without `--check-notice`. Unknown packages,
license-text drift, and SPDX identifiers outside the allow-list all fail closed.

Upstream references:

- [Licensee usage and reports](https://github.com/cashapp/licensee#usage)
- [Licensee allow-list configuration](https://github.com/cashapp/licensee#configuration)
- [SwiftPM resolved dependencies](https://docs.swift.org/swiftpm/documentation/packagemanagerdocs/packageshowdependencies/)
- [MPVKit 0.41.0 licensing](https://github.com/mpvkit/MPVKit/tree/0.41.0#license)
