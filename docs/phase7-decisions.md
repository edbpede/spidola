<!--
SPDX-FileCopyrightText: 2026 Spidola contributors
SPDX-License-Identifier: AGPL-3.0-or-later
-->

# Phase 7 software decisions and evidence

| | |
|---|---|
| **Status** | Accepted for the Phase 7 software-complete branch |
| **Date** | 2026-07-16 |
| **Scope** | Decision-gated 2.0 explorations and release-readiness evidence |

This record closes the three Phase 7 explorations by an explicit ship/defer decision. A checked
exploration in the implementation plan therefore means **resolved**, not that a deferred feature
was implemented. Physical-device, store-account, and release-publication acceptance remains in
Phase 8.

## EPG timeline grid: defer; ship now/next

**Decision:** ship the P1 now/next experience and defer the two-axis timeline grid until after 1.0.

The product contract already separates these scopes: PRD §6.6 makes now/next P1 and the full grid
P2. The implemented core follows that boundary. `crates/core-api/src/services/epg.rs` exposes
`now_next`, bounded per-channel windows, and an atomic batched guide query;
`crates/core-db/src/repo/epg.rs` stores and prunes the rolling source window; and
`crates/core-parse/src/xmltv.rs` incrementally filters XMLTV against an injected window. This is the
right contract for channel rows, details, and the in-playback strip while preserving a bounded
future grid seam.

A TV grid is not just another rendering of the same page. It needs a bounded cross-channel query,
horizontal time virtualization, vertical channel virtualization, stable focus restoration across
both axes, predictable D-pad escape paths, and proof against PRD §9's 100 ms hitch ceiling. The
core query seam no longer requires an N-query fan-out, but a production grid still needs the focus,
virtualization, refresh, and performance evidence below. Shipping the screen before now/next has
been exercised on both shells would spend the remote-navigation and performance budget without
improving the P1 path.

Reconsider the grid only after all of the following are true:

- now/next, programme details, the rolling-window setting, and the channel strip are green on both
  virtual devices;
- the core has one bounded cross-channel window query or equivalent batch contract, with an
  inspected query plan;
- a D-pad prototype proves stable focus when either axis recycles cells and when EPG data refreshes;
- the virtual-device run records no hitch over 100 ms, followed by the Phase 8 hardware check.

## Android local recording: defer; tvOS remains unsupported

**Decision:** do not implement local video recording in Phase 7. Keep it as a separately approved
Android-only P2 project. tvOS non-support is final unless Apple's storage model materially changes.

There is no video-recording implementation in the repository. References such as
`RecentsService::record` and `PlaybackViewModel.recordRecent` record recently watched history, not
media. No remux pipeline, recording foreground service, storage allocation model, partial-file
recovery, or recording-management UI exists. Treating those names as evidence of a recorder would
be a category error.

The Android work needs a design and legal gate before code:

- **Content rights:** Spidola cannot infer whether a user-supplied source may be recorded. A future
  proposal needs maintainer legal/store-policy review, a user-initiated flow, no DRM circumvention,
  no bundled content, and no restreaming. This is scope control, not legal advice.
- **Storage:** recordings need a user-selected destination, free-space and quota handling, atomic
  finalization, recovery or deletion of partial files, and behavior for removed media. Android's
  Storage Access Framework grants access only to the selected tree and warns that persisted access
  does not survive a moved or deleted document: [Android shared-storage guidance](https://developer.android.com/training/data-storage/shared/documents-files).
- **Background execution:** recording must begin from a visible, explicit user action and remain
  user-visible. Android restricts foreground-service starts from the background, and Android 15+
  limits `mediaProcessing` foreground services to six hours in a 24-hour period:
  [background-start restrictions](https://developer.android.com/develop/background-work/services/fgs/restrictions-bg-start)
  and [foreground-service timeouts](https://developer.android.com/develop/background-work/services/fgs/timeout).
  A proposal must define timeout, stop, crash, reboot, and app-update behavior across API 26–36.
- **Media correctness:** remuxing must preserve timestamps and selected tracks without silently
  transcoding, must reject unsupported or encrypted inputs actionably, and must never interfere
  with the sacred playback/zap path.

PRD §6.8 already records the platform divergence: tvOS has purgeable app storage, no honest
user-visible recording destination, and no durable promise the product can make. The tvOS shell
must therefore say that recording is unavailable on Apple TV if a portable export, help page, or
cross-platform description mentions the Android feature; it must not show a disabled control that
implies future parity.

Reopen Android recording only with a reviewed mini-PRD, a Media3/libmpv remux spike against the
deterministic headend, a Storage Access Framework prototype, an API-level foreground-service test
matrix, and explicit legal/store sign-off.

## Platform expansion: defer all phone and tablet ports

**Decision:** keep the supported product surface at Apple TV and Android TV / Google TV through
1.0 and the first P1 release. Do not add phone or tablet targets in Phase 7.

| Surface | Current support | Decision |
|---|---|---|
| Apple TV (tvOS 18+) | Supported shell; MPVKit default, AVPlayer alternate | Continue |
| Android TV / Google TV (API 26+) | Supported shell; ExoPlayer default, libmpv fallback | Continue |
| iPhone / iPad | No application target, touch navigation, compact layout, or mobile playback acceptance | Defer |
| Android phone / tablet | TV manifest, Compose-for-TV focus model, and TV playback acceptance only | Defer |
| Desktop / web | PRD §4 non-goal; no shell | Do not pursue |

The Rust core and engine contract make a later port cheaper, but they do not prove a mobile
product. Touch navigation, lifecycle and picture-in-picture behavior, adaptive layouts, background
audio policy, mobile storage, and separate store review would each require platform-specific work.
Adding nominal targets now would expand the test and support matrix before the TV product reaches
its Phase 8 gate.

Reconsider expansion after 1.0 hardware/store acceptance and one stable P1 release, through a new
PRD amendment that identifies a mobile user problem, chooses one platform first, and funds its own
accessibility, performance, playback, and distribution matrix.

## Phase 7 evidence ledger

### Stale-shell drill

The boundary cut created a genuine frozen-shell case. The current core reports schema 3 from
`crates/core-db/src/migrations.rs` and boundary 7 from `crates/core-api/src/lib.rs`, while the
frozen shells at `e3831cb` still accept only schema 2 / boundary 4:

- tvOS `apps/tvos/App/AppContainer.swift` uses exact equality and terminates with an
  `Incompatible core: ... schema ..., boundary ...` message before bootstrap;
- Android `apps/androidtv/app/src/main/kotlin/dev/spidola/tv/SpidolaApplication.kt` uses exact
  equality and throws an `IllegalStateException` with the same version details before bootstrap;
- `SpidolaApplicationTest` proves stale schema and stale boundary values are rejected.

The exact frozen 2/4 shells were rebuilt with the current 3/7 core artifacts and launched on the
virtual matrix. Android TV terminated in `SpidolaApplication.onCreate` with
`Incompatible core 0.0.0: schema 3, boundary 7`; tvOS terminated in `AppContainer.swift` with
`Incompatible core: 0.0.0, schema 3, boundary 7`. Both failures happened before bootstrap and
included every compared version. The source/unit checks and actual stale-artifact launches now
close the Phase 7 drill; it is repeated against release artifacts in Phase 8.

### Hostile input, benchmarks, and deterministic headend

Fresh host evidence on 2026-07-16:

- `cargo test -p core-pair --test pairing`: 30 passed, including oversized request line/header/body,
  header-count, malformed request, concurrency, and slow-loris cutoff cases;
- `cargo test -p core-parse`: 30 passed across unit and property suites, including arbitrary bytes,
  malformed UTF-8, oversized M3U lines/XMLTV fields, chunk-boundary invariance, and bounded buffers;
- `cargo test -p spidola-test-headend`: 8 integration tests passed for the manifest, byte ranges,
  traversal rejection, unauthorized/unreachable routes, timeout, mid-stream drop, unsupported
  format, and decoder failure.

Criterion targets exist for 50k M3U import, 50k search, and XMLTV. `.github/workflows/core.yml`
builds a base-revision worktree, runs the configured baseline/candidate benchmarks, validates every
estimate, and fails changes beyond the committed tolerance. The parser tests and the regression
gate's Python tests are green.

`tools/test-headend/` is deterministic and self-produced, and `docs/engine-acceptance.md` maps its
routes to the engine taxonomy. The debug-only tvOS app harness ran AVPlayer and MPVKit through the
success route and all six error classes; Android instrumentation mounted real Compose surfaces and
did the same for ExoPlayer and the packaged libmpv/JNI stack. All four adapters reported the exact
contract class. The libmpv emulator run used its explicit software-decoding mode because the AVD's
MediaCodec surface path stalls independently of mpv; production retains MediaCodec-copy decoding
with software fallback by default, and Phase 8 covers that path on physical hardware.
Picture/sound, timing, full codec breadth, and fallback UX remain Phase 8 hardware rows rather than
being inferred from virtual state transitions.

### Direct release, licenses, and changelog

The direct Android release path is implemented without publishing a release: `.github/workflows/release.yml`
builds a universal three-ABI APK, enforces complete signing configuration for tag publication,
creates SHA-256 checksums, generates notes from Conventional Commits, retains dry-run artifacts,
and uploads signed tag artifacts. `apps/androidtv/signing.env.example` documents the local/store
signing inputs. Tags, store signing, and actual publication remain Phase 8.

License enforcement now covers each dependency graph:

- Rust: `cargo deny check` plus REUSE;
- Android: Cash App Licensee's `:app:licenseeRelease` task, with a packaged JSON report;
- tvOS: `tools/licenses/check-swiftpm-licenses.py` checks the resolved graph, reviewed policy, license
  text, and notice drift;
- media stacks: the Android and MPVKit pin-verification scripts enforce the committed LGPL flags.

`tools/release/generate-changelog.py` and the release workflow implement Conventional-Commit
changelog generation. tvOS `AboutView` renders the generated SwiftPM notice and Android
`AboutScreen` renders Licensee's packaged dependency report plus the native-media LGPL text. The
release-wide cargo-deny, per-graph license, pin, and REUSE gates are part of the final verification
ledger, so the combined About/audit item is closed.

### Community translation posture

`crowdin.yml` maps all seven tvOS string catalogs and all seven Android resource catalogs, and
`tools/community/validate-translations.py` is wired into the core CI lane. Local validation passes
with `7 tvOS + 7 Android catalogs; complete shared locales=[]`. There is still no repository
evidence of a live hosted project, a complete community locale, or a first external translation
PR, so the Community checklist remains open. Those require Crowdin credentials and an external
contributor; a configuration file is contribution infrastructure, not a community event.

## Process cleanup contract

Every virtual-device or headend run must leave the host clean, on success or failure:

- start the headend with `tools/test-headend/headend.sh start`, stop it with
  `tools/test-headend/headend.sh stop`, and require `status` to report not running afterward;
- the CI Android runner already installs an `EXIT` trap that calls `adb emu kill`, waits for the
  emulator process, and only then uses a forced kill as a last resort
  (`tools/ci/run-android-tv-emulator-ci.sh`);
- local Android runs must likewise use `adb -s <serial> emu kill` and verify no task-owned emulator
  remains;
- tvOS runs must shut down every simulator booted for the task (`xcrun simctl shutdown <UDID>`, or
  `xcrun simctl shutdown all` when the task owns all booted simulators) and verify that no task-owned
  simulator remains in the `Booted` state.

Cleanup is part of acceptance evidence. A route run with a live headend, emulator, or simulator
left behind is incomplete.
