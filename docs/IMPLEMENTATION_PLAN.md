# Spidola — Implementation Plan

| | |
|---|---|
| **Document status** | Draft v1.1 — July 2026 |
| **Companion documents** | `PRD.md` (scope, priorities) · `TECH_SPEC.md` (architecture, standards) |
| **Coding standards** | `.augment/rules/rust-dev-pro.md` · `.augment/rules/swift-dev-pro.md` · `.augment/rules/kotlin-dev-pro.md` — normative for every task below |
| **Conventions** | Phases are sequential; tasks within a phase may run in parallel unless a dependency is noted. Every phase ends with explicit **exit criteria**. Checkboxes track completion. |

Two standing rules apply to **every** task in this plan and are not repeated per item:
**Error handling** — no code merges with a bare unwrap/expect on a fallible path (Rust), an untyped or swallowed error (Swift), or a caught-and-ignored exception / leaked `Result` across a module boundary (Kotlin); every new failure path maps into the layer's error taxonomy per TECH_SPEC §4.7.
**Logging** — every new subsystem lands with tracing spans (core) or subsystem/category logging (shells) wired into the pipeline per TECH_SPEC §4.8, with secrets provably absent from output.

---

## Phase 0 — Repository, governance, and toolchain bootstrap

- [x] **Repository skeleton**
  - [x] Create the monorepo with the exact tree from TECH_SPEC §3.2 (empty crates/modules with placeholder manifests)
  - [x] Add root `.gitignore` (Rust · Swift/Xcode/SPM · Kotlin/Android/Gradle · macOS); `Cargo.lock` and SwiftPM `Package.resolved` are committed for reproducibility (§9)
  - [x] Add `.augment/rules/` containing the three coding-standard documents
  - [x] Add `LICENSES/` (AGPL-3.0-or-later plus dependency license texts) and REUSE configuration
  - [x] Add SPDX headers to all seed files; wire the REUSE lint
  - [x] Commit `docs/` with PRD, TECH_SPEC, and this plan
- [x] **Governance (launch-blocking, PRD §10)**
  - [x] Decide and document the contributor model (recommendation: DCO + explicit App Store distribution grant)
  - [x] Add CONTRIBUTING with the modularity doctrine summary and the two standing rules above
  - [x] Run the "Spidola" trademark / store-name availability check (original name "Orbita" failed and was replaced; App Store Connect reservation remains the definitive test, tracked in Phase 7)
- [x] **Toolchain pins**
  - [x] `rust-toolchain.toml` pinned to 1.96.1; workspace manifest with resolver 3, edition 2024, `workspace.lints` per rules file
  - [x] `docs/toolchains.md` recording the Xcode/Swift (6.3.x) and Kotlin (2.4.0) / AGP / KSP2 / Gradle pins; build scripts assert them
  - [x] `deny.toml` with the advisory feed and the license allow-list (permissive + LGPL; copyleft-incompatible denied)
- [x] **Local commit hooks (`prek`)**
  - [x] Install `prek` and add `prek.toml` at the repo root (config schema: <https://prek.j178.dev/configuration/>); set `default_install_hook_types` so one `prek install` wires both the pre-commit and commit-msg shims
  - [x] Builtin fast gates: whitespace / EOF / LF line-endings, merge-conflict + case-conflict guards, large-file + private-key detection, JSON/TOML/YAML validation, and no-commit-to-`main`
  - [x] Conventional Commits check on the commit-msg stage; gitleaks secret scan (TECH_SPEC §12)
  - [x] Path-scoped local gates mirroring the three CI lanes (§9): Rust `cargo fmt` + `clippy -D warnings` + `cargo deny` under `crates/`; tvOS `swift-format` + SwiftLint under `apps/tvos/`; Android TV `ktlint` + `detekt` (Compose ruleset) under `apps/androidtv/`
  - [x] Keep local gates to fast format/lint only — full Swift/Kotlin compilation, simulator/emulator smoke tests, and the REUSE lint stay CI-side; document `prek install` and `prek run --all-files` in CONTRIBUTING
- [x] **CI skeleton (three lanes, TECH_SPEC §9)**
  - [x] Core lane: rustfmt, clippy (deny-warnings), test, cargo-deny, REUSE lint
  - [x] Android lane: Gradle build, ktlint, detekt (+ Compose ruleset), unit tests
  - [x] Apple lane: Xcode build, swift-format, SwiftLint, Swift Testing
  - [x] Advisory complexity/length lints configured at **warn** per the modularity doctrine (never CI-failing alone)

**Exit criteria:** empty-but-real projects build green in all three lanes; REUSE and cargo-deny pass; `prek run --all-files` passes on the seed tree; governance decisions documented.

---

## Phase 1 — Rust core foundations

- [x] **`core-model` — domain types**
  - [x] Newtype identifiers; `Source` enum (m3u-url / m3u-file / xtream); `Channel`, `Category`, `EpgEntry`, `Favorite`, `PlaybackHistoryEntry`
  - [x] Validated stream-locator type ("parse, don't validate" constructor)
  - [x] Secret types: redacted Debug, zeroize-on-drop, no serde on raw values; unit tests proving redaction
- [x] **Error taxonomy scaffolding (first, not last)**
  - [x] Per-crate thiserror v2 error enums with source chains, stubbed for every crate in this phase
  - [x] The flattened FFI-facing error enum drafted in `core-api` with variant-to-UX mapping table cross-checked against PRD §6.3
- [x] **Logging pipeline scaffolding (first, not last)**
  - [x] `tracing` initialized in `core-api` with target-per-crate convention; ring buffer subscriber for export
  - [x] CI grep guarding against Debug-formatting of secret types in log macros
- [x] **`core-db` — persistence**
  - [x] Connection pool (WAL, single writer / multiple readers); rusqlite bundled
  - [x] Numbered forward-only migrations (rusqlite_migration); migration test harness (every historical schema → head)
  - [x] Repositories: sources, channels, favorites, history, settings (one file each per §3.2)
  - [x] FTS5 search index with trigger maintenance; contentless-delete configuration
  - [x] Staging-and-swap refresh transaction with fault-injection property test (fail at any point → prior catalog intact)
  - [x] Stable per-source channel identity hash so favorites/hidden survive refresh
- [x] **`core-fetch` — HTTP**
  - [x] reqwest + rustls client construction; timeouts (connect / read / overall deadline); redirect hop cap
  - [x] Per-source user-agent and header injection
  - [x] Streaming body → sink adapter (no full buffering)
  - [x] Per-source self-signed-TLS escape hatch, off by default, unit-tested for scoping
- [x] **`core-parse` — M3U (streaming)**
  - [x] Line/state-machine lexer; extinf attribute handling with unknown-attribute preservation
  - [x] Batch sink trait; bounded-memory invariant benchmarked (peak ≈ one batch regardless of input size)
  - [x] Diagnostics ledger (skipped-entry accounting) surfaced in import results
  - [x] Encoding sniffing with UTF-8-lossy fallback
  - [x] Property tests: random mutation of fixtures never panics; accounting invariant holds
  - [x] Seed `fixtures/m3u/` golden corpus with provenance notes
- [x] **`core-search`**
  - [x] Prefix query compilation over FTS5; source/type filters; trigram fuzzy fallback ranking
  - [x] Criterion benchmark at a generated 50k-channel dataset against the 50 ms budget

**Exit criteria:** a CLI-driven integration test (via `xtask`) imports a 50k-channel fixture from a local HTTP stub into SQLite within budget, searches it under 50 ms, and survives fault-injected refresh — all under the phase's error and logging rules.

---

## Phase 2 — FFI boundary and packaging

- [x] **`core-api` façade**
  - [x] Owned Tokio multi-thread runtime; blocking-adapter discipline for all `core-db` calls
  - [x] Services (one file each): source, catalog (paged-by-contract), search, favorites, settings — Xtream and pairing stubbed
  - [x] Task-handle pattern: quick return + progress/completion/failure via callback listener; honest cancellation at batch boundaries
  - [x] Startup handshake reporting core version, schema version, boundary version
- [x] **UniFFI (proc-macro mode)**
  - [x] Records/enums for the domain surface; async methods throughout; callback interfaces for events, secrets, and the log sink
  - [x] Threading contract documented on every callback interface (may arrive on any thread)
  - [x] `xtask` targets: generate Swift + Kotlin bindings; drift check for CI
- [x] **Packaging**
  - [x] XCFramework build for aarch64-apple-tvos + simulator (Tier 2 stable toolchain; nightly build-std fallback documented)
  - [x] cargo-ndk AAR/prefab for arm64-v8a, armeabi-v7a, x86_64
  - [x] Reproducibility check in CI (rebuild bindings, fail on drift)
- [x] **Contract tests (parity keel)**
  - [x] Minimal Swift and Kotlin harnesses executing the same fixture flows against the real compiled core, asserting identical results
  - [x] Panic-across-FFI detector: any core panic in contract tests is a red build
  - [x] Error-mapping tests: every FFI error variant constructed and asserted representable on both sides

**Exit criteria:** both shells (bare test harnesses, no UI yet) import a fixture playlist through the boundary, receive progress callbacks, cancel mid-import, and log through their sink — with identical observable results.

---

## Phase 3 — Walking-skeleton apps (Milestone M0)

- [x] **Shared design tokens**
  - [x] Encode the PRD §8 palette, type scale, spacing, and focus treatment as tokens in both DesignSystem modules
  - [x] Focus appearance components (Test-Card Amber treatment riding platform focus behavior)
- [x] **tvOS shell**
  - [x] App target as composition root; SPM local packages per §3.2; Swift 6 language mode + default-MainActor isolation everywhere
  - [x] CoreKit: UniFFI wrapper, main-actor trampolining for callbacks, Keychain-backed secrets callback, OSLog sink (subsystem/category/privacy per §4.8)
  - [x] State-driven navigation stack; FeatureBrowse rendering a fixture channel list with correct focus traversal
- [ ] **Android TV shell**
  - [ ] Single-Activity app module; Hilt wiring; version catalog; Navigation 3 back-stack-as-state
  - [x] corekit: UniFFI wrapper, coroutine/Flow adapters with end-to-end cancellation, Keystore-backed secrets callback, tagged logcat sink
  - [x] feature:browse rendering a fixture channel list using tv-material components, foundation lazy lists, focus-restorer, pivot scrolling
- [ ] **CI completion**
  - [ ] Emulator D-pad traversal smoke test (Android)
  - [x] Simulator unit/state + D-pad traversal smoke tests (tvOS)
  - [ ] Both apps run on real reference hardware (manual checklist recorded)

**Exit criteria (= M0):** CI green on all targets; both apps browse a fixture catalog on real hardware with correct focus behavior; logs from core and shell interleave coherently in each platform's tooling.

---

## Phase 4 — Sources, catalog, and search (toward M1)

- [ ] **Add-source flows (both platforms)**
  - [ ] M3U by URL with live progress, cancellation, and diagnostics summary ("N channels, M skipped")
  - [ ] M3U from local file (document picker / SAF / paste)
  - [ ] Source list: rename, disable, refresh, delete; refresh preserves favorites/hidden (identity-hash test on device)
  - [ ] Per-source auto-refresh interval setting
- [ ] **Actionable-error UX (PRD §6.3 discipline)**
  - [ ] Error-presentation component mapping every FFI variant to plain-language class + prescribed actions; snapshot/UI tests over the full variant set
  - [ ] "No action available" is unrepresentable in the component's API
- [ ] **Browse completion**
  - [ ] Source → type → category → channel drill-down; virtualized everywhere; scroll-hitch profiling pass on the low-end Android baseline
  - [ ] Logo pipeline: lazy load, placeholder, capped disk cache (Coil / URLSession pipeline)
  - [ ] Context menu: play, favorite, hide, details, per-channel engine override (engine option stubbed until Phase 5)
- [ ] **Search UI**
  - [ ] Global search reachable everywhere; per-keystroke results against the core budget; source/type filters
  - [ ] Remote text entry + platform phone-keyboard input flow verified on hardware
- [ ] **Favorites + recents**
  - [ ] Favorites row first on home; recents with purge toggle and off switch

**Exit criteria:** the self-hoster persona can add a real playlist by URL on both platforms, browse and search it fluidly on reference hardware, and every induced failure (bad URL, 401, garbage file, mid-import network drop) presents an actionable error and a clean log trail.

---

## Phase 5 — Playback (Milestone M1 lands here)

- [ ] **Player contract (both platforms, before any engine)**
  - [ ] Contract interface + state machine + EngineError taxonomy exactly per TECH_SPEC §8, in PlayerContract / player-contract
  - [ ] Engine selection policy (channel → source → platform default) with unit tests
  - [ ] Contract-level fake engine for feature-code tests
- [ ] **Android: Media3/ExoPlayer engine (default)**
  - [ ] Compose player surface (media3-ui-compose); HLS/DASH/TS coverage; hardware decode verification matrix
  - [ ] Media session integration (system remote + voice transport)
  - [ ] Track selection, buffering profiles, error mapping into EngineError
- [ ] **tvOS: MPVKit engine (default)**
  - [ ] Metal-backed hosting view; mpv property/command mapping; event stream → contract state machine
  - [ ] Audio session for long-form playback; interruption handling (Siri, handoff); suspension teardown/rebuild acceptance test
  - [ ] Now-playing info reporting
- [ ] **tvOS: AVPlayer engine (alternate)** — contract wrapper for HLS-native streams
- [ ] **Android: libmpv engine (fallback)**
  - [ ] Pinned LGPL libmpv per-ABI build in `tools/build-libmpv-android/`, checksummed
  - [ ] SurfaceView rendering; JNI lifecycle hardening; error mapping
- [ ] **Playback UX**
  - [ ] Click-to-first-frame instrumented against the 2 s budget (both platforms, default engines)
  - [ ] Info overlay; audio/subtitle selection; aspect cycling; subtitle appearance settings
  - [ ] **Zap path**: D-pad up/down channel flip; engine teardown/rebuild profiled as the sacred path
  - [ ] **Channel strip** (the PRD §8.5 signature): lower-third with adjacent-channel peek, SMPTE ribbon, one-frame appearance, timeout/back dismissal
  - [ ] Loud fallback: UnsupportedFormat/DecoderFailed → "Try other player" + remember-for-channel toggle; engine transitions logged per §4.8
- [ ] **Engine acceptance suite**
  - [ ] Maintainer test headend serving self-produced streams per EngineError class; per-release manual checklist committed

**Exit criteria (= M1):** a household member watches a channel unaided on both platforms; zap and channel strip meet budgets on reference hardware; forcing each EngineError class produces the correct loud-fallback or actionable error on all four engines.

---

## Phase 6 — Xtream, pairing, settings, accessibility (completing P0)

- [ ] **`core-xtream`**
  - [ ] Auth handshake; live/VOD/series catalogs; series → seasons/episodes expansion; defensive wire deserialization
  - [ ] Centralized, audited stream-URL credential embedding; scrubbed fixture corpus + stub-server tests
  - [ ] Secrets flow: credentials via host-secrets callback only; DB stores opaque keys (verified by test inspecting the DB file)
- [ ] **Xtream in the apps** — add-account flow, series browsing UI, per-source refresh semantics, 401-renewal error path
- [ ] **`core-pair` + pairing UX**
  - [ ] LAN-only server, alive only while its screen is visible; session-random token; single static form + single POST shape
  - [ ] AGPL §13 source link on every served page
  - [ ] TV screen with QR + URL + token; submission lands as a pre-filled add-source flow
- [ ] **Settings (full PRD §6.9 surface)**
  - [ ] All settings wired through the core SettingsService; defaults verified ("usable without opening settings" walkthrough)
  - [ ] Diagnostics screen: log level (runtime tracing filter), log export (ring buffer, redaction test on export output), versions incl. core git revision
- [ ] **Accessibility + localization baseline**
  - [ ] VoiceOver / TalkBack pass over every focusable element; reduce-motion honored; contrast audit against tokens
  - [ ] String extraction complete; localization infrastructure live; English strings copy-edited per PRD §8.6 voice

**Exit criteria:** all PRD P0 features function on both platforms; secrets provably never touch SQLite or logs; the app passes a full screen-reader walkthrough.

---

## Phase 7 — Hardening and release (Milestone M2 / 1.0)

- [ ] **Performance verification** — every PRD §9 budget measured on reference + low-end hardware; criterion regression gates locked; Instruments / Macrobenchmark+Perfetto reports archived per release checklist
- [ ] **Soak and abuse testing** — 24 h playback soak per default engine; hostile-input pass over parsers/pairing (oversized lines, malformed UTF-8, slow-loris on pairing server)
- [ ] **Release engineering**
  - [ ] Signed store pipelines; Android direct-release fat APK with checksums attached to GitHub releases
  - [ ] Third-party notices generated into About; final cargo-deny/REUSE audit; LGPL build flags for mpv/FFmpeg committed and verified
  - [ ] Conventional-commit changelog generation
- [ ] **Store submission (PRD §10 posture)**
  - [ ] Reserve the app name in App Store Connect (create the app record) — the definitive "Spidola" availability test (PRD §13); maintainer action
  - [ ] Content-neutral listings; one-page privacy policy; reviewer demo source on the maintainer headend (self-produced/public-domain streams only)
  - [ ] Play TV form-factor checklist (banner, D-pad completeness); App Store submission with appeal plan documented
- [ ] **1.0 tag** — versioned schema + boundary handshake verified against a deliberately stale shell (fails fast and legibly)

**Exit criteria (= M2):** store approvals or documented appeals in flight; direct-distribution artifacts published; all budgets green in the archived reports.

---

## Phase 8 — P1 fast-follow (Milestone M3)

- [ ] **EPG (now/next)** — XMLTV streaming parser with rolling-window pruning (`core-parse/xmltv`); Xtream EPG endpoints; background incremental ingest with bounded storage; now/next on channel rows and in the channel strip
- [ ] **Custom channels** — create/edit (name, URL, logo, headers/UA); groups; portable export/import (the cross-device answer, PRD §6.7)
- [ ] **Platform surfaces** — tvOS Top Shelf extension (app-group snapshot); Android home-screen channels / watch-next; Android system content-search provider
- [ ] **Personalization** — user-arrangeable favorites ordering
- [ ] **Community** — translation platform live; first community locales shipped; contributor docs validated by the first external PR (governance model from Phase 0 exercised)

**Exit criteria (= M3):** all P1 features shipped in a 1.x release on both platforms with parity per PRD §7.

---

## Phase 9 — 2.0 explorations (Milestone M4, decision-gated)

- [ ] **EPG timeline grid** — virtualized two-axis grid within TV performance budgets
- [ ] **Recording (Android only, PRD §6.8)** — remux-to-storage while watching; storage management UX; explicit non-support messaging on tvOS
- [ ] **Platform expansion review** — assess phone/tablet ports now that core + contract are proven

**Exit criteria:** each item resolved by shipped feature or documented decision; no silent backlog.
