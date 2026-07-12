# Spidola — Implementation Agent Prompt

> Reusable prompt for implementing a single task (or full phase) of the Spidola implementation plan.
> Append the target task identifier under **Task to Implement** at the bottom.

---

## Instructions

You are implementing a task from the **Spidola** project — a free, open-source (AGPL-3.0-or-later) IPTV player built exclusively for the living room, natively on **tvOS** and **Android TV / Google TV**. It ships no content, no accounts, no telemetry, no ads — ever. Users bring their own sources (M3U URL, M3U file, Xtream Codes account); the app delivers obsessive speed (sub-2 s click-to-first-frame, sub-50 ms search at 50k channels), D-pad-first navigation a household member can operate unaided, and mpv-class codec breadth. Architecture is three concentric layers: a **Rust core** owning all non-UI logic (parsing, persistence, search, fetching, EPG, settings), a **UniFFI binding layer**, and two **native shells** owning navigation, focus, rendering, and the player engines. The core is the single source of truth; the shells hold no durable business state.

The stack is split three ways:

- **Rust core** — Rust 1.96.1, edition 2024, Cargo virtual workspace (resolver 3): rusqlite (bundled, FTS5) + rusqlite_migration, reqwest + rustls, Tokio multi-thread runtime owned by `core-api`, thiserror v2, `tracing`, UniFFI (proc-macro mode); tooling rustfmt + clippy + cargo-deny + REUSE.
- **tvOS shell** — Swift 6.3+ (Swift 6 language mode, strict concurrency, default-MainActor isolation), SwiftUI + Observation, state-driven NavigationStack, SPM local packages; players MPVKit (default) and AVPlayer; tooling swift-format + SwiftLint + Swift Testing.
- **Android TV shell** — Kotlin 2.4 (K2-only), Jetpack Compose for TV (`androidx.tv:tv-material` on foundation lazy layouts), Navigation 3, Hilt (KSP2), Gradle Kotlin DSL + version catalog; players Media3/ExoPlayer (default) and libmpv; tooling ktlint + detekt (Compose ruleset).

### Step 1: Read and Analyze the Task

Read `docs/IMPLEMENTATION_PLAN.md` and locate the exact section matching the task identifier provided at the end of this prompt. Phases are headed `## Phase N — Title` and are **sequential**; tasks within a phase are top-level bold checklist items (e.g. `- [ ] **\`core-db\` — persistence**`) and may run in parallel unless a dependency is noted. Phase exits map to PRD milestones: Phase 3 = M0, Phase 5 = M1, Phase 7 = M2/1.0, Phase 8 = M3, Phase 9 = M4. Thoroughly analyze:

- **All subtasks** (nested checklist items `- [ ]`) under that task, and the phase's closing `**Exit criteria:**` line — a task is done only when its subtasks are checked **and** the exit criteria it contributes to are satisfiable.
- **The two standing rules** at the top of the plan — they apply to *every* task and are not repeated per item; violations are review blockers:
  - **Error handling** — no bare unwrap/expect on a fallible path (Rust), no untyped or swallowed error (Swift), no caught-and-ignored exception or leaked `Result` across a module boundary (Kotlin); every new failure path maps into the layer's error taxonomy per TECH_SPEC §4.7.
  - **Logging** — every new subsystem lands with tracing spans (core) or subsystem/category logging (shells) wired into the pipeline per TECH_SPEC §4.8, with secrets provably absent from output.
- **Dependencies** on prior tasks/phases — phases are strictly sequential, and several tasks are load-bearing for later work (e.g. Phase 1's error-taxonomy and logging scaffolding come *first, not last*; the Phase 2 FFI contract tests underpin the parity policy; the Phase 5 player contract precedes any engine). Check that prerequisite items are marked `- [x]`. If any required dependency is incomplete, **stop and report which dependencies are missing** before proceeding.
- **Budgets, gotchas, and notes** embedded in the task — PRD `§`-references, performance numbers (they are requirements, not aspirations), parenthetical notes, and decisions explicitly deferred to a later phase.
- **Cross-references** to the PRD's non-goals (§4), platform parity policy (§7), UX direction (§8), quality bars (§9), and licensing/compliance (§10), and to the TECH_SPEC's modularity doctrine (§3.1), monorepo tree (§3.2), error/logging policies (§4.7/§4.8), FFI rules (§5), engine contract (§8), and security/license engineering (§12) — all of which constrain this task's implementation.

### Step 2: Read Project Coding Standards

Read the rules files in `.augment/rules/` in full — `rust-dev-pro.md` for core work, `swift-dev-pro.md` for tvOS work, `kotlin-dev-pro.md` for Android TV work; all that apply to the task. They are **normative, not advisory** (TECH_SPEC §1); every line of code you produce **must** comply. Where TECH_SPEC and a rules file conflict, the conflict is a bug in one of them, resolved by amending the offending document — the sanctioned divergences already on record are listed under Project Invariants below. Key requirements (non-exhaustive — the full documents govern, including their anti-pattern tables):

**Rust core (1.96.1 / edition 2024)**

- **Workspace conventions** — virtual manifest, resolver 3, `workspace.package` (edition 2024, `license = "AGPL-3.0-or-later"`), shared versions in `workspace.dependencies`, workspace `[lints]` (clippy pedantic at warn priority −1, the project deny-set, complexity/length lints at warn) that every member crate opts into explicitly.
- **Errors** — per-crate error enums with **thiserror v2** and source chains; `anyhow` only in `xtask` and tests, never in library crates. **Never `.unwrap()`/`.expect()` on a fallible path**; `expect` only for documented invariants. Panics are bugs; a panic crossing the FFI is a release blocker.
- **Async discipline** — Tokio is the runtime, owned by `core-api`, invisible to the shells. Never block async worker threads; all `core-db` entry points are blocking functions that only the service layer may call through the runtime's blocking adapter. Never hold a `std::sync` guard across `.await`.
- **Ownership** — no reflexive `.clone()`, no default `Arc<Mutex<T>>`/`Rc<RefCell<T>>`; least-owning parameters (`&str`, `&[T]`); newtypes over bare primitives (SourceId, ChannelId); "parse, don't validate" constructors; iterator chains over index loops.
- **Modules** — modern layout only, **`mod.rs` is banned**; private by default, `pub(crate)` for internal cross-module items, deliberate re-exported surface per crate root.
- **Traits** — native `async fn in traits`/RPITIT; `#[async_trait]` only where `dyn` dispatch forces it; static dispatch preferred; abstraction must be earned (one implementation = no trait, except the sanctioned engine contract).
- **Unsafe** — last resort, `// SAFETY:` comment on every use, `#[forbid(unsafe_code)]` where possible; edition-2024 `unsafe extern`/`unsafe(...)` attribute forms.
- **Testing** — cargo test + **proptest** (parsers must never panic on mutated fixtures) + **criterion** (budget benchmarks with regression gates); golden fixture corpus under `fixtures/` with provenance notes.
- **Tooling** — `cargo fmt --all`, `cargo clippy --all-targets -- -D warnings`, `cargo deny check`; no OpenSSL anywhere in the tree (rustls only).

**tvOS shell (Swift 6.3 / SwiftUI)**

- **Swift 6 language mode everywhere** — strict concurrency as an error; app/UI targets use default-MainActor isolation, so anything crossing to the core is an explicit, compiler-checked boundary. **Never `@unchecked Sendable`** to silence the compiler — use an actor, truly immutable `final class`, or `sending`.
- **Structured concurrency only** — async/await throughout; never completion handlers, `DispatchQueue`/GCD, or Combine. UniFFI callbacks may arrive on any thread — CoreKit trampolines them to the main actor; `withCheckedContinuation` for unavoidable legacy bridges.
- **Observation framework** — UI state is `@Observable final class`; **never `ObservableObject`/`@Published`/`@StateObject`/`@ObservedObject`**. `@State` owns, `@Bindable` binds, `@Environment` shares.
- **Value semantics first** — default `struct`/`enum`; classes `final`; `[weak self]` + `guard let self` in stored escaping closures. **Never force-unwrap or `try!` in production.**
- **Navigation** — state-driven `NavigationStack` + typed route enum + `navigationDestination(for:)`; never `NavigationView`.
- **Persistence** — **SwiftData is deliberately not used** (TECH_SPEC §6 overrides the rules doc's default): persistence is the core's job; the only Apple persistence surfaces are Keychain (secrets) and the image disk cache. The only shell-side networking is the URLSession artwork pipeline.
- **Structure** — SPM local packages per TECH_SPEC §3.2 (CoreKit, DesignSystem, PlayerContract, PlayerMPV, PlayerAV, FeatureX per slice); app target is a composition root only.
- **Testing & tooling** — **Swift Testing** (`@Test`/`#expect`) for units, XCTest for UI tests only; toolchain-bundled `swift format` (+ `swift format lint --strict`) with SwiftLint as the complementary CI linter; OSLog `Logger` with subsystem/category/privacy redaction — never `print` for diagnostics.

**Android TV shell (Kotlin 2.4 / Compose for TV)**

- **K2-only, Gradle Kotlin DSL + `libs.versions.toml`**; `compilerOptions {}` (never the removed `kotlinOptions {}`); **KSP2, never KAPT**; Hilt via KSP; the `org.jetbrains.kotlin.plugin.compose` plugin.
- **Null safety & types** — **never `!!`**; sealed hierarchies with exhaustive `when` (no `else` on sealed); `@JvmInline value class` for typed ids; `kotlinx.serialization` over Jackson/Gson; read-only collection interfaces by default; `ImmutableList` across composable boundaries.
- **Coroutines** — never `GlobalScope.launch`, never `runBlocking` in production; suspend functions main-safe; **`CancellationException` is always rethrown, never swallowed** — cancellation propagates end-to-end (departed screen → scope → core task handle). Blocking calls wrapped in `withContext(Dispatchers.IO)`.
- **Flow discipline** — expose `.asStateFlow()`/`.asSharedFlow()`, never public mutable flows; `_state.update { }` for atomic updates; cold `Flow` from adapters, hot `StateFlow` from ViewModels.
- **Compose for TV** — interactive components from **`androidx.tv.material3`, never phone `material3`**; the removed `TvLazy*` family is never referenced (foundation `Lazy*` + pivot via bring-into-view spec); **Leanback is banned**; `Modifier.focusRestorer()` on scrollable containers **with stable lazy keys**; `requestFocus()` only inside `LaunchedEffect` on an attached `FocusRequester`; Navigation 3 with back stack as plain state; Media3 Compose `PlayerSurface`, never `PlayerView` in `AndroidView`; release players in `DisposableEffect.onDispose`.
- **Testing & tooling** — kotlin.test + JUnit 5, **MockK (never Mockito)**, `runTest` + `StandardTestDispatcher` (virtual time), Turbine for flows, Compose UI tests for focus traversal; ktlint + detekt with the Compose ruleset.

**All layers**

- **Modularity doctrine (TECH_SPEC §3.1)** — one unit, one reason to change; split at concept boundaries on evidence, never on a size counter; **no junk drawers** (`utils`, `helpers`, `misc`, `manager` are banned names); vertical feature slices in the apps (browse, playback, sources, search, settings), never technical-kind piles; features depend downward, never sideways; composition only at each app's shell and `core-api`'s constructor path; abstraction must be earned. Complexity/length lints run at warn and never fail CI alone — answer them in review with "is this one concept?".
- **The two app trees mirror each other unit for unit** (CoreKit ↔ corekit, PlayerContract ↔ player-contract, feature slices one-to-one); engines are peers injected by the composition root, never children of the playback feature.
- **Licensing** — SPDX header (`AGPL-3.0-or-later`) on every file, REUSE-compliant; dependency licenses within the cargo-deny allow-list (permissive + LGPL; copyleft-incompatible denied).
- **Commits** — Conventional Commits (enforced by the `prek` commit-msg hook); never commit directly to `main`.

### Step 3: Read Existing Codebase Context

Before writing any code:

1. Use `codebase-retrieval` (the primary code-search tool — semantic, always reflects disk) to find all types, modules, and functions referenced by the task. Batch related symbols into a single detailed query.
2. Read any files that will be modified or extended (e.g. root `Cargo.toml`, `rust-toolchain.toml`, `deny.toml`, `prek.toml`, crate manifests, `apps/tvos/Packages/*/Package.swift`, `apps/androidtv/settings.gradle.kts`, `gradle/libs.versions.toml`, existing `crates/*`, `apps/*`, and `.github/workflows/`).
3. Understand the monorepo layout established by TECH_SPEC §3.2 (`crates/core-{model,parse,xtream,db,fetch,search,pair,api}` + `xtask`, `apps/tvos/` with local SPM packages, `apps/androidtv/` with `app`/`core`/`player`/`feature` modules, `fixtures/`, `tools/`) and keep new code inside the correct module boundary.
4. Check for existing tests that cover related functionality (Rust unit/property/criterion suites per crate; the FFI contract-test harnesses; Swift Testing suites; kotlin.test/Compose suites) — and whether the `fixtures/` corpus already models the input you need.
5. Reference `docs/TECH_SPEC.md` for architectural context: the layer model (§2), workspace conventions (§3.3), core crate designs (§4), the FFI boundary rules (§5), the shell architectures (§6–7), the engine contract (§8), CI lanes (§9), testing strategy (§10), performance engineering (§11), and security/license engineering (§12). Reference `docs/PRD.md` for product behavior: feature requirements (§6), parity policy (§7), UX/remote mapping and the channel strip (§8), and quality bars (§9).
6. Treat the TECH_SPEC §14 decision log as settled — do not relitigate its decisions in code.

### Step 4: Implement the Task

For each subtask (checklist item):

1. **Plan** the implementation approach before writing code.
2. **Write** the code with full file paths, following the module layout and naming conventions already established.
3. **Explain** any non-obvious architectural decisions inline (brief comments where logic isn't self-evident — no unnecessary doc comments). Decisions the plan or spec flags as significant are captured by amending the relevant doc (PRD/TECH_SPEC §14 decision log).
4. **Test** — write corresponding tests in the correct tier: Rust unit tests for pure logic (parsers, ranking, selection policy), proptest for parser robustness and the staging-swap fault-injection property, criterion for budgeted paths, migration tests for schema changes; FFI contract tests (both bindings, same fixture, identical results) for boundary changes; Swift Testing / kotlin.test for view-model logic against a fake CoreKit; Compose/simulator UI smoke tests for focus behavior.
5. **Verify** — run and report the lanes the task touches:
   - Core: `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, `cargo deny check`, REUSE lint
   - tvOS: Xcode build, `swift format lint --strict --recursive`, `swiftlint`, Swift Testing suites
   - Android: Gradle build, ktlint, detekt (with Compose ruleset), unit tests
   - Repo-wide: `prek run --all-files`

#### Code Quality Rules

- One bounded concern per crate/package/module/file; keep pure domain logic (parsers, query compilation, ranking, engine-selection policy, the engine state machine) free of I/O and platform imports so it is unit-testable in isolation — parsers take an injected "now" for the EPG window, never a clock.
- Model closed sets as enums/sealed hierarchies with exhaustive matching: Rust enums in the core, typed Swift error enums with exhaustive switches, Kotlin sealed classes with exhaustive `when`. Enums crossing the FFI reserve an "unknown future variant" arm on the shell side.
- Validate external/untrusted data at system boundaries. Wire deserialization of Xtream responses is defensive (numbers-as-strings, missing fields — tested against scrubbed real-world fixtures); M3U parsing is tolerant (skip-and-count, never fail the import); Spidola's own domain types use "parse, don't validate" constructors so illegal states don't construct.
- FFI records are flat, owned data; every I/O-touching service method is async; every list-returning method is **paged by contract** (offset/limit or cursor) — no unbounded collections ever cross the boundary.
- No stringly-typed identifiers or bool flags — newtypes and enums; no `Any`-equivalents leaking (Rust `dyn` only where earned, Swift `any` minimized, Kotlin platform types never trusted as non-null).
- **No adjacent-ecosystem habits**: no server-side async-SQL patterns in the core (rusqlite is the decided approach); no UIKit-first or Combine patterns on tvOS; no phone-Material, Leanback, or `TvLazy*` idioms on Android (all three rules files end with an anti-pattern section — treat the "Wrong" column as forbidden).

#### Project Invariants (violations are defects, not style issues)

- **The core is the single source of truth** for all persisted data; shells cache nothing durable except images and player-engine internals. Business state never accumulates in a shell.
- **Playback lives in the shells, not the core** (TECH_SPEC §14); both platforms implement the shared engine contract (TECH_SPEC §8) with the fixed EngineError taxonomy (SourceUnreachable, Unauthorized, UnsupportedFormat, DecoderFailed, Timeout, Unknown-with-detail). Selection policy: per-channel override → per-source override → platform default. **Automatic fallback is loud, never silent**: only UnsupportedFormat/DecoderFailed trigger the one-button "Try other player" (+ remember-for-channel toggle).
- **Secrets never touch SQLite or logs** (TECH_SPEC §12): Xtream credentials and token-bearing headers flow only through the host-secrets callback (Keychain / Keystore-backed prefs); the DB stores opaque keys; secret types redact Debug, zeroize on drop, and never serde-serialize raw values.
- **Parsers are streaming and memory-bounded**: peak parser memory ≈ one batch regardless of playlist size; bytes flow network → parser → DB batch with no full buffering. This is what makes 50k channels honest on 1 GB devices.
- **All HTTP lives in `core-fetch`** (reqwest + rustls; no OpenSSL). The one sanctioned exception is shell-side artwork fetching (public logo URLs via platform image pipelines); authed artwork routes through a core resolver.
- **Blocking work never sits on async worker threads**: `core-db` entry points are blocking functions callable only via the service layer's blocking adapter.
- **FFI discipline** (TECH_SPEC §5): callbacks may arrive on any thread — shells trampoline; a panic crossing the FFI is a release blocker; the versioned startup handshake (core/schema/boundary) must fail fast and legibly on mismatch. Long operations return a task handle quickly; progress/completion/failure arrive via listener; cancellation is honest (checked at batch boundaries).
- **Refresh can never corrupt**: channel refresh is staging-and-swap inside one transaction (failure at any point leaves the prior catalog intact); favorites/hidden survive refresh via the stable per-source identity hash, never row ids.
- **Migrations are forward-only and numbered**; a downgraded app refuses a newer schema with a clear message. No `create_all`-style shortcuts.
- **The pairing server** binds LAN-only, exists only while its screen is visible, serves one static form + one POST shape, requires the session-random token, and renders the AGPL §13 source-code link on every page.
- **Performance budgets (PRD §9) are requirements**: cold start < 1.5 s; click-to-first-frame < 2 s (HLS, default engine); search < 50 ms at 50k channels; 50k-channel import < 30 s on the **low-end Chromecast-class baseline** (not the Shield); zero >100 ms scroll hitches. The channel-zap path is sacred — profiled every release.
- **Errors are always actionable** (PRD §6.3): every user-visible error maps to a plain-language failure class with prescribed actions (retry, try other player, go back); an error with no action is a design bug. Full diagnostic chains go to the log stream, not the FFI error. No system jargon reaches the screen — users manage *sources* and *channels*, never *playlists parsed* or *FFI errors* (PRD §8.6).
- **D-pad first, always**: predictable focus order, unmistakable focus treatment (Test-Card Amber per platform idiom), focus never trapped or lost on data refresh, TV-safe margins everywhere, remote mapping per the PRD §8.4 table.
- **Platform parity is the default** (PRD §7); divergence requires a documented platform constraint — the sanctioned list is recording (Android-only), system content-search (Android-only), default engine (MPVKit tvOS / ExoPlayer Android), and Top Shelf vs. home-screen channels.
- **Accessibility is P0 baseline**: screen readers on every focusable element, reduce-motion honored (all motion < 200 ms and suppressible), WCAG AA contrast per the PRD §8.2 palette, no text below caption focusable.
- **No telemetry, no phone-home, no third-party SDKs with network behavior, ever** (PRD §4/§10); data never leaves the device except to fetch the user's own sources.
- **Licensing is engineering**: AGPL-3.0-or-later SPDX headers + REUSE on every file; mpv/FFmpeg pinned to LGPL configurations with build flags committed; cargo-deny license allow-list enforced in CI.
- **No feature may depend on unshipped platform capability or another feature sideways** — anything two features need moves down a layer explicitly.

### Step 5: Update the Implementation Plan

After completing each subtask, update `docs/IMPLEMENTATION_PLAN.md` on disk:

- Change `- [ ]` to `- [x]` for each completed item (including nested items) under the task you implemented.
- Leave the two standing rules and the `**Exit criteria:**` lines alone — exit criteria are satisfied by evidence, not checked off.
- Do **not** modify any other parts of the document.
- Do **not** mark items complete unless the code is actually written, tested, and the relevant quality gates pass. If you are blocked — a missing dependency, a product decision the docs do not resolve (PRD §13 records every previously open question as resolved; do not reopen them, but halt on any genuinely new ambiguity), or a platform behavior contradicting the TECH_SPEC — leave the item unchecked, halt, and escalate.

### Step 6: Summary Report

After all work is done, provide a concise summary:

```
## Completed
- [x] Item 1 description
- [x] Item 2 description
...

## Files Created/Modified
- `path/to/file.rs` — description of what was added/changed
...

## Verification
- Commands run (cargo fmt / clippy / test / deny, REUSE; swift format lint / swiftlint / Swift Testing; gradle build / ktlint / detekt / tests; prek run --all-files) and their results
- Phase exit criteria progress (cite the plan's Exit criteria line and the two standing rules)

## Architectural Decisions
- Decision 1: rationale
...

## Deviations from Plan
- None (or: description + justification)

## Next Steps
- What tasks/phases are now unblocked
- Any newly surfaced product ambiguities (PRD §13 is fully resolved; flag anything new for the maintainer)
```

---

## Task to Implement

<!-- Append the task identifier below. Examples:
     Implement: ## Phase 0 — Repository, governance, and toolchain bootstrap
     Implement: Phase 1 · **`core-parse` — M3U (streaming)**
     Implement: Phase 2 · **Contract tests (parity keel)**
     Implement: Phase 5 · **Player contract (both platforms, before any engine)**
-->
