<!--
SPDX-FileCopyrightText: 2026 Spidola contributors
SPDX-License-Identifier: AGPL-3.0-or-later
-->

# Spidola test headend

This repository-owned HTTP origin supports the real-engine acceptance matrix in
`docs/engine-acceptance.md`. It is maintainer tooling, is not linked into either app, and serves
only media generated from FFmpeg's `testsrc2` and `sine` filters. Generated media lives under
`target/`; no third-party or copyrighted programme content is downloaded or committed.

## Requirements

- The repository Rust toolchain.
- `curl` for the background-start readiness check.
- An LGPL-configured FFmpeg for fixture generation. `generate-assets.sh` refuses FFmpeg builds
  configured with `--enable-gpl` or `--enable-nonfree` and never selects `libx264` or `libx265`.
- LGPL-compatible H.264 High, HEVC Main10, and VP9 encoders. On macOS the default H.264 and HEVC
  encoders are VideoToolbox. Override selection with `SPIDOLA_H264_ENCODER`,
  `SPIDOLA_HEVC_ENCODER`, or `SPIDOLA_VP9_ENCODER` when needed.

## Lifecycle

From the repository root:

```sh
tools/test-headend/headend.sh generate
tools/test-headend/headend.sh start
tools/test-headend/headend.sh status
tools/test-headend/headend.sh stop
```

`start` writes its PID and log to `target/test-headend-runtime/`, waits until
`/manifest.json` responds, and fails cleanly if startup does not complete. `stop` sends `SIGTERM`,
waits for exit, and removes the PID file. Use `run` instead of `start` for a foreground server.

The default listener is `0.0.0.0:8090`, while generated manifest URLs use
`http://127.0.0.1:8090`. Override `SPIDOLA_HEADEND_PUBLIC_BASE` for the client being tested:

```sh
# Android emulator host loopback
SPIDOLA_HEADEND_PUBLIC_BASE=http://10.0.2.2:8090 tools/test-headend/headend.sh start

# A physical device on the same network
SPIDOLA_HEADEND_PUBLIC_BASE=http://192.0.2.10:8090 tools/test-headend/headend.sh start
```

The Apple simulator can use `http://127.0.0.1:8090`. A physical device needs the development
Mac's reachable LAN address and local-firewall access to port 8090.

Useful overrides are listed by running `tools/test-headend/headend.sh` without a subcommand.
`SPIDOLA_HEADEND_DURATION_SECONDS` defaults to 60. The failure timing defaults are 300 seconds
for `/timeout` and 20 seconds for `/mid-stream-drop`.

## Routes

`GET /manifest.json` is the machine-readable entry point and contains absolute URLs for every
success stream and failure route.

| Route | Deterministic headend behavior | Expected engine result |
|---|---|---|
| `/streams/...` | Serves the seven generated success fixtures with single-range byte seeking. | `Playing`, subject to the documented AVPlayer VP9 exception. |
| `/unreachable` | Redirects to the reserved, non-resolving `spidola.invalid` domain. | `SourceUnreachable` |
| `/unauthorized` | Returns `401` with `WWW-Authenticate`. | `Unauthorized` |
| `/forbidden` | Returns `403`. | `Unauthorized` |
| `/unsupported-format` | Serves ZIP signature bytes as `video/mp2t`. | `UnsupportedFormat` |
| `/decoder-failed` | Preserves the first third of the H.264 TS, then corrupts later packet payloads. | `DecoderFailed` |
| `/timeout` | Sends a complete `200` response header and no body for the configured stall period. | `Timeout` |
| `/unknown` | Returns the deliberately unclassified status `520` with an expectation header. | `Unknown` fallback |
| `/mid-stream-drop` | Declares the full TS length, throttles one third of it, then closes the socket. | Engine-specific failure, never `Ended` |

`Unknown` is intentionally tested separately from recognized mappings: it proves that an
unclassified engine failure remains diagnostic rather than being mislabeled as a recognized
class. If a platform normalizes status 520 before the engine adapter sees it, record the observed
native error and use that evidence to adjust the platform acceptance mapper or fixture.

## Host verification

The tests synthesize tiny placeholder assets and do not invoke FFmpeg:

```sh
cargo test -p spidola-test-headend
cargo clippy -p spidola-test-headend --all-targets -- -D warnings
```

They cover the route manifest, status/authentication headers, reserved-domain redirect, disguised
archive, deterministic post-lead corruption, header-only stall, truncated content length, static
asset MIME types, byte ranges, traversal rejection, and graceful server shutdown.
