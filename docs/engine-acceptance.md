<!--
SPDX-FileCopyrightText: 2026 Spidola contributors
SPDX-License-Identifier: AGPL-3.0-or-later
-->

# Engine acceptance suite

| | |
|---|---|
| **Applies to** | All four engines: MPVKit (tvOS default), AVPlayer (tvOS alternate), ExoPlayer (Android default), libmpv (Android fallback) |
| **Cadence** | Every release, before tagging (TECH_SPEC §10) |
| **Companion documents** | `TECH_SPEC.md` §8 (engine contract), §10 (testing strategy), §11 (performance) · `PRD.md` §6.3 (actionable errors), §9 (budgets) |

Everything below the engine contract is unit-tested and runs in CI: the selection policy, the
EngineError taxonomy, every engine's error mapping, and the playback view models against the
contract's fake engine. **This document covers the part that cannot be**: whether a real decoder,
on real hardware, against a real stream, produces the state machine the contract promises.

TECH_SPEC §10 names this honestly as "manual-with-checklist per release, the least automatable
layer". The checklist exists so that honesty does not become an excuse.

## 1. The test headend

A maintainer-operated origin serving **only self-produced or public-domain streams** (PRD §10 —
the same content-neutrality posture that governs the store reviewer demo source). It exists to
induce each EngineError class on demand, which no third-party playlist can be relied on to do.

It is not part of the app, ships with nothing, and is never referenced from shipped code.

The deterministic implementation is `tools/test-headend/`. It generates every success stream
from FFmpeg's `testsrc2` and `sine` sources and writes all binary media beneath the ignored
`target/test-headend-assets/` directory. It downloads no content and commits no generated media.

From the repository root, prepare and start it with:

```sh
tools/test-headend/headend.sh generate
tools/test-headend/headend.sh start
```

The route manifest is `http://127.0.0.1:8090/manifest.json`. The Apple simulator can use that
address directly. For Android Emulator, restart with
`SPIDOLA_HEADEND_PUBLIC_BASE=http://10.0.2.2:8090`; physical devices use the development Mac's
reachable LAN address. See `tools/test-headend/README.md` for timing, encoder, binding, and
firewall overrides. Always clean up the repository-owned process after the run:

```sh
tools/test-headend/headend.sh stop
```

### 1.1 Streams that must play

One per row, each ≥ 60 s, generated with an LGPL FFmpeg build (matching our own configuration —
`tools/build-mpvkit/`, `tools/build-libmpv-android/`):

| Id | Container / protocol | Video | Audio | Why it is here |
|---|---|---|---|---|
| `hls-h264-aac` | HLS (fMP4) | H.264 High | AAC-LC | The baseline. The click-to-first-frame budget is measured on this one (PRD §9). |
| `hls-hevc-eac3` | HLS (fMP4) | HEVC Main10 | E-AC-3 | Hardware decode + passthrough path. |
| `dash-h264-aac` | DASH | H.264 | AAC-LC | Android's second protocol. |
| `ts-mpeg2-mp2` | MPEG-TS over HTTP | MPEG-2 | MP2 | The classic IPTV shape; the reason mpv-class breadth is a goal. |
| `ts-h264-aac` | MPEG-TS over HTTP | H.264 | AAC-LC | The most common real-world IPTV stream. |
| `mkv-vp9-opus` | Matroska over HTTP | VP9 | Opus | Codec breadth beyond the Apple-native set — the case that separates MPVKit from AVPlayer. |
| `hls-multi-audio-subs` | HLS | H.264 | AAC ×3 + WebVTT ×2 | Track enumeration and selection. |

### 1.2 Routes that must fail, each in exactly one way

The taxonomy has six classes (TECH_SPEC §8). Every one needs a route that induces it reliably —
an error class you cannot induce is an error class you have never tested.

| Route | Induces | How |
|---|---|---|
| `/unreachable` | `SourceUnreachable` | Redirects to `spidola.invalid`, a reserved DNS name that cannot resolve. |
| `/unauthorized` | `Unauthorized` | Returns `401` with `WWW-Authenticate`. `/forbidden` returns `403`. |
| `/unsupported-format` | `UnsupportedFormat` | Serves ZIP signature bytes as `video/mp2t`. |
| `/decoder-failed` | `DecoderFailed` | Preserves the leading third of a valid TS, then corrupts later packet payloads. |
| `/timeout` | `Timeout` | Sends complete `200` headers, then no body for 300 s. |
| `/unknown` | `Unknown` | Returns deliberately unclassified status `520`; validates the diagnostic catch-all rather than a recognized mapping. |
| `/mid-stream-drop` | engine-specific | Declares the full TS length, serves one third over 20 s, then closes — checks the engine does not report `Ended` for a live drop. |

The exact headend behavior above is host-tested. Real-engine classification remains part of this
manual matrix because platform networking layers may normalize errors before an adapter sees
them. `/unknown` is the intentional exception to the normal "not Unknown" assertion below: it
proves the catch-all remains available and diagnostic.

## 2. Per-release checklist

Run the full matrix on **reference hardware** and the **low-end Chromecast-class Android
baseline** (PRD §9 — the Shield is not the baseline and never was). Record the build, the device,
and the date. A row is only green with a real observation behind it.

### 2.1 Playback matrix — each engine × each stream

For each engine (MPVKit, AVPlayer, ExoPlayer, libmpv) × each §1.1 stream:

- [ ] Video reaches the screen and advances.
- [ ] Audio is present and in sync.
- [ ] The state machine reports `Playing` (not stuck in `Buffering`).
- [ ] `hls-multi-audio-subs`: every audio track and subtitle track is listed, selectable, and the
      selection takes effect. Subtitles turn off.
- [ ] Aspect cycles Fit → Fill → Stretch and each is visibly distinct.

AVPlayer is expected to fail `mkv-vp9-opus` with `UnsupportedFormat` — **that is a pass**, and it
is precisely the case the loud fallback exists for. Record it as such rather than as a defect.

### 2.2 Error matrix — each engine × each §1.2 route

For each engine × each failure route:

- [ ] The engine reports **the class in the table**. For recognized routes, an unexpected
      `Unknown` is a mapping bug: fix the mapping, do not amend the table. `/unknown` must report
      `Unknown`, with retained diagnostic detail.
- [ ] The screen shows the plain-language failure class and at least one action (PRD §6.3). An
      error with no action is a design bug.
- [ ] No system jargon reaches the screen — no codec names, no HTTP codes, no engine names
      (PRD §8.6).
- [ ] The diagnostic chain **is** in the log stream (TECH_SPEC §4.8).
- [ ] **No secret appears in the log**: no header values, no user-agent tokens, no credential-
      bearing URL (TECH_SPEC §12). Grep the captured log for the account password before signing
      off.

### 2.3 Loud fallback

On each platform, with the default engine:

- [ ] `/unsupported-format` → "Try other player" is offered.
- [ ] `/decoder-failed` → "Try other player" is offered.
- [ ] `/unreachable`, `/unauthorized`, `/timeout` → it is **not** offered (another engine would
      fail identically; offering it would waste the viewer's time).
- [ ] Accepting it plays on the alternate engine.
- [ ] "Remember for this channel" → the channel opens on the alternate engine next time, **and
      still does after a source refresh** (the choice is keyed on the stable identity hash, not a
      row id).
- [ ] "Just this once" → the next open is back on the default.
- [ ] Nothing ever switches engine without the viewer pressing the button. A silent swap is a
      release blocker, not a nicety (TECH_SPEC §8).

### 2.4 Budgets (PRD §9)

Measured on `hls-h264-aac`, default engine, release build, warm app:

- [ ] **Click-to-first-frame < 2 s.** The model logs every measurement with its budget; read it
      from the log stream rather than a stopwatch.
- [ ] **Zap** (D-pad up/down): video from the new channel within the same 2 s, and the UI never
      blocks. Profile it — this is the sacred path (TECH_SPEC §11), and it is profiled every
      release on both platforms.
- [ ] Zap 50 times in a row: no drift upward, no leaked memory, no leaked decoder. Engines are
      destroyed and rebuilt on every flip; a leak here is invisible at 5 zaps and fatal at 500.
- [ ] The channel strip appears in one frame and never stalls video (PRD §8.5).
- [ ] Reduce-motion on: the strip still appears and dismisses, with no slide (PRD §8.6).

### 2.5 Lifecycle

- [ ] Suspend mid-playback and resume: playback rebuilds or fails legibly — never a black screen
      that claims to be playing.
- [ ] tvOS: Siri interruption pauses, and dismissing it resumes.
- [ ] tvOS: now-playing info shows the channel; the remote's transport works.
- [ ] Android: the media session responds to the system remote and to voice transport.
- [ ] Back during playback: dismisses the strip first, then leaves (PRD §8.4).
- [ ] Leaving the screen mid-load cancels the load — the departed screen's engine is released and
      its core task cancelled end-to-end.

## 3. Recording the run

Commit the completed checklist to the release PR, naming build, devices, and date. A failed row
blocks the release or is filed with an issue number written next to it. An empty checklist and an
un-run checklist look identical six months later — which is the whole reason this file is in the
repository rather than in someone's head.
