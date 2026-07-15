<p align="center">
  <img src="../spidola-logo.svg" alt="Spidola Logo" width="160" height="160">
</p>

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
- [x] **Android TV shell**
  - [x] Single-Activity app module; manual constructor composition accepted for M0; version catalog; Navigation 3 back-stack-as-state
    - Post-M0 production hardening: migrate the composition root to Hilt/KSP2 as the dependency graph grows
  - [x] corekit: UniFFI wrapper, coroutine/Flow adapters with end-to-end cancellation, Keystore-backed secrets callback, tagged logcat sink
  - [x] feature:browse rendering a fixture channel list using tv-material components, foundation lazy lists, focus-restorer, pivot scrolling
- [x] **CI completion**
  - [x] Emulator D-pad traversal smoke test (Android)
  - [x] Simulator unit/state + D-pad traversal smoke tests (tvOS)
  - [x] Both apps pass their local virtual-device runtime checklist (Android TV emulator + tvOS Simulator)
  - [x] Updated Android native-build + emulator workflow passes on GitHub Actions
  - Physical Android TV and Apple TV hardware runs are deferred and non-blocking for M0 until
    suitable devices are available; retain them as later validation for hardware-specific behavior

**Exit criteria (= M0):** CI green on all targets; both apps browse a fixture catalog on the Android
TV emulator and tvOS Simulator with correct focus behavior; logs from core and shell interleave
coherently in each platform's tooling. Physical-device validation is deferred and does not block M0.

---

## Phase 4 — Sources, catalog, and search (toward M1)

- [x] **Add-source flows (both platforms)**
  - [x] M3U by URL with live progress, cancellation, and diagnostics summary ("N channels, M skipped")
  - [x] M3U from local file (document picker / SAF / paste)
  - [x] Source list: rename, disable, refresh, delete; refresh preserves favorites/hidden (identity-hash regression plus live-catalog emulator pass)
  - [x] Per-source auto-refresh interval setting
  - [x] M3U source URLs plus channel/history locators and credential-bearing headers are authenticated-encrypted at rest; M3U identities use a catalog-keyed HMAC rather than exposing a public verifier for credential URLs; the catalog key lives in the platform secure store (crash-retry-safe schema 2 cutover and raw SQLite/WAL regression)
- [x] **Actionable-error UX (PRD §6.3 discipline)**
  - [x] Error-presentation component mapping every FFI variant to plain-language class + prescribed actions; snapshot/UI tests over the full variant set
  - [x] "No action available" is unrepresentable in the component's API
- [x] **Browse completion**
  - [x] Source → type → category → channel drill-down; virtualized everywhere
  - [ ] Scroll-hitch profiling on the low-end Android hardware baseline — Phase 7 performance acceptance
  - [x] Logo pipeline: lazy load, placeholder, capped disk cache (Coil / URLSession pipeline)
  - [x] Context menu: play, favorite, hide, details, per-channel engine override (engine option stubbed until Phase 5)
- [x] **Search UI**
  - [x] Global search reachable everywhere; per-keystroke results against the core budget; source/type filters
  - [x] Remote text entry and platform phone-keyboard integration implemented; remote text entry verified on virtual devices
  - [ ] Platform phone-keyboard input verified with a physical phone/TV pair — Phase 7 hardware acceptance
- [x] **Favorites + recents**
  - [x] Favorites row first on home; recents with purge toggle and off switch

**Exit criteria:** the self-hoster persona can add a real playlist by URL on both platforms, browse and search it fluidly on reference hardware, and every induced failure (bad URL, 401, garbage file, mid-import network drop) presents an actionable error and a clean log trail. The complete functional flow is verified on both virtual devices (checkpoint below); reference/low-end hardware performance and phone-keyboard acceptance remain Phase 7 work.

---

## Phase 5 — Playback (Milestone M1 lands here)

- [x] **Player contract (both platforms, before any engine)**
  - [x] Contract interface + state machine + EngineError taxonomy exactly per TECH_SPEC §8, in PlayerContract / player-contract
  - [x] Engine selection policy (channel → source → platform default) with unit tests
  - [x] Contract-level fake engine for feature-code tests
- [x] **Android: Media3/ExoPlayer engine (default)**
  - [x] Compose player surface (media3-ui-compose); HLS/DASH/TS coverage; hardware decode verification matrix
    - The decode matrix is specified and committed (`docs/engine-acceptance.md` §2.1); **running it needs
      reference hardware and the test headend** and is deferred with the other hardware validation
  - [x] Media session integration (system remote + voice transport)
  - [x] Track selection, buffering profiles, error mapping into EngineError
- [x] **tvOS: MPVKit engine (default)**
  - [x] Metal-backed hosting view; mpv property/command mapping; event stream → contract state machine
  - [x] Audio session for long-form playback; interruption handling (Siri, handoff); suspension teardown/rebuild acceptance test
  - [x] Now-playing info reporting
- [x] **tvOS: AVPlayer engine (alternate)** — contract wrapper for HLS-native streams
- [x] **Android: libmpv engine (fallback)**
  - [x] Pinned LGPL libmpv per-ABI build in `tools/build-libmpv-android/`, checksummed
  - [x] SurfaceView rendering; JNI lifecycle hardening; error mapping
- [x] **Playback UX**
  - [x] Click-to-first-frame instrumented against the 2 s budget (both platforms, default engines)
    - Instrumented and logged against the budget on every load; **measuring it on reference hardware**
      is deferred with the other hardware validation
  - [x] Info overlay; audio/subtitle selection; aspect cycling; subtitle appearance settings
  - [x] **Zap path**: D-pad up/down channel flip; engine teardown/rebuild profiled as the sacred path
    - Teardown/rebuild is unit-tested (previous engine disposed, ring resolved in one paged query);
      **profiling on reference hardware** is deferred with the other hardware validation
  - [x] **Channel strip** (the PRD §8.5 signature): lower-third with adjacent-channel peek, SMPTE ribbon, one-frame appearance, timeout/back dismissal
  - [x] Loud fallback: UnsupportedFormat/DecoderFailed → "Try other player" + remember-for-channel toggle; engine transitions logged per §4.8
- [ ] **Engine acceptance suite**
  - [ ] Maintainer test headend serving self-produced streams per EngineError class; per-release manual checklist committed
    - [x] Per-release manual checklist committed (`docs/engine-acceptance.md`), including the headend
          stream/route specification
    - [ ] Headend stood up and the checklist run on hardware — **maintainer action**, blocks the M1 exit

**Exit criteria (= M1):** a household member watches a channel unaided on both platforms; zap and channel strip meet budgets on reference hardware; forcing each EngineError class produces the correct loud-fallback or actionable error on all four engines.

---

## Phase 6 — Xtream, pairing, settings, accessibility (completing P0)

- [x] **`core-xtream`**
  - [x] Auth handshake; live/VOD/series catalogs; series → seasons/episodes expansion; defensive wire deserialization
  - [x] Centralized, audited stream-URL credential embedding; scrubbed fixture corpus + stub-server tests
  - [x] Secrets flow: credentials via host-secrets callback only; DB stores opaque keys (verified by test inspecting the DB file)
    - Catalogs persist a **credential-free** locator; the password is embedded only at play time,
      in `core-xtream/src/urls.rs` (the audited point). Xtream buffers each listing whole rather
      than streaming — the protocol returns one unpaginated JSON array per listing, so there is
      nothing to stream; bounded by a 64 MiB cap. Revisit against the low-end baseline in Phase 7.
- [x] **Xtream in the apps** — add-account flow, series browsing UI, per-source refresh semantics, 401-renewal error path
  - `SourceService::add_xtream` **verifies the account before storing it**, so a wrong password is a
    sentence on the add screen rather than a mystery on the next refresh; the 401-renewal path is
    the same `Unauthorized` variant with the same prescribed action (re-enter credentials)
  - **Series browsing needed no code**: episodes arrive as channels with `MediaKind::SeriesEpisode`
    and `core-xtream` writes the show name into `group_title`, so the existing
    source → kind → group → channel drill-down already reads source → Series → show → episodes
  - **Play-time resolution was the missing keel**: nothing called `resolve_stream`, so an Xtream
    channel imported, browsed and favorited perfectly and then could not play — the catalog stores
    a credential-free locator by design. Both shells now resolve immediately before handing a
    stream to an engine (per play, never cached), kind-agnostic so the zap path never branches
- [x] **`core-pair` + pairing UX**
  - [x] LAN-only server, alive only while its screen is visible; session-random token; single static form + single POST shape
    - Locality is enforced as a **peer check** (private / link-local / loopback), not merely a bind
  - [x] AGPL §13 source link on every served page
    - One shared page shell ends in a quiet colophon, so a page cannot be served without the
      offer; a test enumerates every renderable page and asserts it
  - [x] TV screen with QR + URL + token; submission lands as a pre-filled add-source flow
    - **Each shell supplies the TV's LAN address itself** (`NWInterface` on tvOS,
      `NetworkInterface` on Android — not `WifiManager`, which is Wi-Fi-only and deprecated from
      API 31). The core's own inference reads the route out of the host, which a full-tunnel VPN
      defeats; both shells prefer `eth*`/`wlan*` so a tunnel cannot win
    - **The submission never travels on the Android back stack**: `rememberNavBackStack` is
      serialized into saved instance state, so a payload there would have written an Xtream
      password to disk. It goes through a one-slot in-memory handoff whose `take()` empties it, so
      a submission pre-fills exactly once; the password field is `remember`, not `rememberSaveable`
    - QR is `zxing` on Android and CoreImage's built-in `CIQRCodeGenerator` on tvOS — an
      implementation asymmetry, not a §7 divergence, since both shells show one. The Android test
      renders the matrix to pixels and decodes it with a real reader: a matrix of the right shape
      that no camera can read would pass every structural check
- [x] **Settings (full PRD §6.9 surface)**
  - [x] All settings wired through the core SettingsService; defaults verified ("usable without opening settings" walkthrough)
    - The typed vocabulary + defaults (`core-api/src/settings.rs`) replace Phase 2's opaque
      key/value FFI surface, so a shell cannot invent an untyped setting; a contract test asserts a
      fresh install resolves every setting with no stored row. Both shells render the same IA: a
      grouped `SpidolaRow` list showing each setting's current value, with nine closed-set settings
      routing through one reusable picker that hands back a **typed** value
    - The **EPG window is deliberately not surfaced**: it is in the core vocabulary because §6.9
      lists it, but ingest is Phase 8 and a control that does nothing is a UX bug
  - [x] Diagnostics screen: log level (runtime tracing filter), log export (ring buffer, redaction test on export output), versions incl. core git revision
    - `set_log_level` persists **and** applies the live filter (and is re-applied at startup),
      `export_logs` snapshots the ring, the handshake reports the core git revision. Redaction on
      export is asserted end-to-end against a headend that mirrors the password back. "Export" is an
      on-screen viewer on both platforms — tvOS has no user-visible file system, and parity is the
      default (PRD §7)
- [x] **Accessibility + localization baseline**
  - [x] Accessibility semantics pass over every focusable element; reduce-motion honored; contrast audit against tokens
  - [ ] Full VoiceOver / TalkBack walkthrough on physical TV hardware — Phase 7 accessibility acceptance
    - **Reduce-motion is done and was a real bug**: `SpidolaFocusRing` (tvOS) and `SpidolaFocus`
      (Android) both animated the focus lift unconditionally, so every focusable surface in the app
      moved even with animations switched off — older than this phase, and failing the P0 bar for
      every slice. Fixed in the shared token on both platforms (only the movement goes; the amber
      border stays, since an invisible focus ring is the worse failure). Android's comment showed
      the misconception outright — "kept under the reduce-motion-safe ceiling (< 200 ms)" conflates
      duration with suppression
    - **State is announced as state, not as part of the name**: the sweep over browse, search, and
      playback follows the idiom the settings slices established — `.accessibilityLabel` +
      `.accessibilityValue` on tvOS, `stateDescription` on Android — because a row that reads
      "Recents, On" as one blob is not what a VoiceOver or TalkBack user expects to hear. The
      favorite/hide and filter-chip surfaces announce their *current* state rather than only the
      verb that would change it. Decorative glyphs (`★`, `✓`, `▲▼`) are cleared from the
      accessibility tree: they are ornament, and a screen reader spelling them out is noise
    - **The contrast audit found a real failure, and this is why it was worth running.** The palette
      was expected to pass everywhere; every pair listed in PRD §8.2 does. But Stream Red — the one
      semantic color that carries *prose* (the add-source validation message on both shells, and
      Material's `error` role on Android) — was `#C0554E` and reached only **4.05:1** on Studio and
      **3.58:1** on Set against the 4.5:1 floor for body text. §8.2 pins hexes for its five named
      values and asks only for a *muted red in the same tonal family*, so the hex was ours: it is
      now `#C96E69`, same hue and saturation, lightness 53% → 60%, at **5.16:1** and **4.56:1**.
      Recorded in TECH_SPEC §14 with the rejected alternatives. Full table, all passing:

      | fg | bg | ratio | needs |
      |---|---|---|---|
      | Broadcast White | Studio | 15.91:1 | 4.5:1 |
      | Broadcast White | Set | 14.06:1 | 4.5:1 |
      | Static | Studio | 5.98:1 | 4.5:1 |
      | Static | Set | 5.28:1 | 4.5:1 |
      | Studio | Test-Card Amber | 8.43:1 | 4.5:1 |
      | Test-Card Amber | Studio | 8.43:1 | 3:1 (non-text) |
      | Test-Card Amber | Set | 7.45:1 | 3:1 (non-text) |
      | Stream Red | Studio | 5.16:1 | 4.5:1 |
      | Stream Red | Set | 4.56:1 | 4.5:1 |
      | Stream Green | Studio | 6.21:1 | 3:1 (icon) |

    - **Focus behaviour remains inspection-verified, deliberately.** The labels are a code pass
      against established idioms; a per-feature-module instrumentation harness that could assert
      focus *restoration* is Phase-7-sized and is not in this phase. The existing XCUITest smoke
      and the Android smoke test stay as they are, and this note says so rather than letting the
      tick imply coverage that does not exist
  - [x] String extraction complete; localization infrastructure live; English strings copy-edited per PRD §8.6 voice
    - **Infrastructure is live on both** (`Localizable.xcstrings`, `strings.xml`), English-first, and
      the sweep now covers every slice: tvOS `FeatureSources`, `FeatureBrowse`, `FeaturePlayback`,
      `FeatureSearch`, and the two interpolation formats in `DesignSystem` (localized as *formats*,
      so word order can vary by language); Android `feature:browse`, `feature:playback`,
      `feature:search`. Counts pluralize through the catalogs rather than through concatenation.
      Enum labels reaching UI resolve through feature-side `@Composable` resolvers, so no resource
      landed in corekit or player-contract
    - **`defaultLocalization` is the silent-echo trap**: a Swift package without it compiles, runs,
      and shows the key instead of the string. Every newly-resourced package carries it
    - **Three view-model channels are deliberately left in English** and are not a sweep:
      `AddSourceViewModel.validation`, `SourcesViewModel.status`, and `ChannelDetailViewModel.notice`
      (found during this pass, same shape as the two already recorded) carry sentences, and some
      interpolate, so resourcing them means restructuring a view-model API to carry resource ids
      plus args — a design change, decided separately
    - **`ActionableError` cannot be localized by a sweep, and this is the reason:**
      `ApiError::InvalidInput` carries `reason: String` — **English prose generated in Rust** — which
      the shells put straight into the message. Resourcing the shell wrappers would localize every
      arm *except the one that varies*, which is worse than not doing it, because it would look
      done. Fully localizing means the core returns an **error code plus structured data** and the
      shell renders the sentence: a TECH_SPEC §5 boundary change across both shells, three slices
      each, and the core's taxonomy. **That question is now answered — the shells own the
      vocabulary (TECH_SPEC §14) — and the implementation is a scoped follow-up, not this PR.** The
      *entire* surface stays unextracted meanwhile (failureClass, message, and the "Try again" /
      "Go back" / "Edit" action labels), so the one place visible English remains is deliberate and
      reads as pending work rather than as an oversight
  - [ ] Finish the structured error-code boundary and the three documented view-model localization channels before claiming a fully localizable 1.0 surface

**Exit criteria:** all PRD P0 features function on both platforms; plaintext credentials provably never touch SQLite or logs; the app passes a full screen-reader walkthrough. Virtual-device P0 functionality and the expanded secret boundary are verified below; the physical screen-reader walkthrough and documented localization boundary work remain open.

### Pre-Phase-7 validation checkpoint — 2026-07-15

- [x] **Real IPTV functional pass on both virtual platforms**
  - [x] Imported an 860-entry M3U catalog on tvOS 26.5 Simulator and an Android API 36 TV AVD
  - [x] Exercised import, source/category/channel browsing, live playback, zap, favorite, search, settings, diagnostics, refresh, and favorite persistence
  - [x] Used Computer Use for the tvOS journeys, including a signed post-security-fix fixture drill-down through stream resolution; Android's raw QEMU window is not addressable by Computer Use, so its visible journey used ADB plus Compose instrumentation
- [x] **Full automated regression matrix after repairs**
  - [x] Rust: 275 tests; rustfmt; strict Clippy
  - [x] tvOS: 187/187 signed simulator unit/UI tests; strict swift-format and SwiftLint
  - [x] Android: 189 JVM tests; lint, ktlint, detekt, debug + instrumentation builds; 4/4 API 36 emulator tests; all three packaged ABIs
  - [x] Swift and Kotlin real-core FFI harnesses pass at schema 2 / boundary 4; generated bindings have no drift
- [x] **Validation-found defects repaired and regression-locked**
  - [x] Removed the API-33 Java Cleaner requirement from generated Android bindings and added root Android lint to CI
  - [x] Fixed Android Add Source D-pad/IME traversal and retained-screen source/home reloads
  - [x] Made Android secret fields memory-only across activity recreation; made Keystore writes/deletes fail closed; made instrumentation data ownership and emulator-only execution explicit
  - [x] Defined deterministic initial tvOS Home focus and made the UI smoke wait for focus settlement without weakening its assertions
  - [x] Removed direct stored-locator playback fallbacks; both shells must resolve locators plus per-channel user-agent/header overrides through the core before constructing an engine
  - [x] Authenticated-encrypted all M3U source/channel/history credential material at rest; added catalog-keyed channel identities, strict domain-separated envelopes, and opaque resolved-stream/header FFI objects whose native diagnostics cannot reflect plaintext
  - [x] Made the schema-2 legacy-row/page scrub crash-retry-safe with a durable pending/complete marker, and proved credentials stay out of SQLite/WAL/logs
  - [x] Made Android refuse any core other than schema 2 / boundary 4 before bootstrap, matching the startup-handshake contract already enforced by tvOS
- [ ] **Hardware/headend acceptance that virtual devices cannot close**
  - [ ] Stand up the deterministic maintainer headend and run `docs/engine-acceptance.md` across all four engines and every EngineError/decode route
  - [ ] Measure click-to-first-frame, zap teardown/rebuild, scroll hitching, startup, and series-heavy Xtream peak memory on reference and low-end hardware
  - [ ] Verify physical phone-keyboard/pairing-camera flows plus VPN and multi-interface address selection
  - [ ] Verify hardware decode/codec coverage, Siri/interruption handling, AirPlay, audio/subtitle behavior, and physical-remote semantics
  - [ ] Complete the physical VoiceOver/TalkBack walkthrough and the remaining localization boundary work above

**Checkpoint result:** Phase 7 can begin with all automatable pre-Phase-7 emulator/simulator work green. The open items are deliberately carried into Phase 7 because they require physical hardware, a deterministic headend, performance measurement, or the already-scoped localization boundary change; they do not masquerade as completed acceptance.

---

## Phase 7 — Hardening and release (Milestone M2 / 1.0)

- [ ] **Performance verification** — every PRD §9 budget measured on reference + low-end hardware; criterion regression gates locked; Instruments / Macrobenchmark+Perfetto reports archived per release checklist
- [ ] **Hostile-input testing** — exercise parsers and pairing with oversized lines, malformed UTF-8, and slow-loris behavior
- [ ] **Release engineering**
  - [ ] Signed store pipelines; Android direct-release fat APK with checksums attached to GitHub releases
  - [ ] Third-party notices generated into About; final cargo-deny/REUSE audit; LGPL build flags for mpv/FFmpeg committed and verified
  - [ ] **Close the license-gate gap: cargo-deny only audits the Rust graph.** The JVM/Gradle graph
        (Media3, Compose, Hilt, JNA, zxing) and the SPM graph (MPVKit) have no automated license
        gate — `android.yml` and `apple.yml` run no license step at all, so `deny.toml`'s allow-list
        has never applied to them and a shell dependency's license is a reviewer's job. Found in
        Phase 6 when zxing was added and `cargo deny check` was (wrongly) treated as evidence about
        it. Add an allow-list-or-fail gate per graph — `app.cash.licensee` is the Gradle analogue —
        so "all bundled components must be AGPL-compatible" (PRD §10) is enforced rather than
        asserted (TECH_SPEC §12)
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
