<!--
SPDX-FileCopyrightText: 2026 Spidola contributors
SPDX-License-Identifier: AGPL-3.0-or-later
-->

# Stream fixtures

The maintainer-operated engine-acceptance headend is implemented in `tools/test-headend/`.
Its seven success streams are generated entirely from FFmpeg's synthetic video and audio sources:

```sh
tools/test-headend/headend.sh generate
```

Generated manifests and media are written to `target/test-headend-assets/`, which is ignored by
Git. The repository therefore contains the reproducible recipe and route contract, not binary
media. The generator performs no downloads and no copyrighted programme content is committed.

Run `tools/test-headend/headend.sh start`, then open `/manifest.json` on the configured headend
base URL for the absolute success-stream and failure-route manifest. Always finish a manual run
with `tools/test-headend/headend.sh stop`.
