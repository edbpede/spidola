# Contributing

Thank you for considering a contribution. This project is free software, licensed **AGPL-3.0-or-later**, built to be a pleasure to contribute to. This document covers the legal terms and the two engineering rules that apply to every change.

## Contributor terms — read before your first PR

There is **no CLA** and no copyright assignment: you keep the copyright to your work. Every contribution is submitted under two grants:

1. **AGPL-3.0-or-later** — the project license (`LICENSE`).
2. **The App Store Distribution Exception** (`APPSTORE_EXCEPTION.md`) — an additional permission under AGPL section 7 that keeps distribution through Apple's App Store lawful once the code has more than one copyright holder. It adds a permission for one channel and takes nothing away; the app remains AGPL for everyone.

You certify both by agreeing to the [Developer Certificate of Origin 1.1](https://developercertificate.org/) and signing off each commit:

```sh
git commit -s
```

This appends a `Signed-off-by: Your Name <you@example.com>` line to the commit message. A local `prek` hook checks for it, and the DCO check on pull requests is the authoritative gate — unsigned commits cannot merge.

The full decision and rationale are recorded in `docs/adr/0001-contributor-model.md`.

## The two standing rules

These apply to **every** task and PR; violations are review blockers (see `docs/IMPLEMENTATION_PLAN.md`):

- **Error handling** — no code merges with a bare unwrap/expect on a fallible path (Rust), an untyped or swallowed error (Swift), or a caught-and-ignored exception / leaked `Result` across a module boundary (Kotlin). Every new failure path maps into the layer's error taxonomy per TECH_SPEC §4.7.
- **Logging** — every new subsystem lands with tracing spans (core) or subsystem/category logging (shells) wired into the pipeline per TECH_SPEC §4.8, with secrets provably absent from output.

## The modularity doctrine (summary)

The goal is cohesion — neither god-files nor confetti code. The full doctrine is TECH_SPEC §3.1; the principles reviews enforce without appeal:

- **One unit, one reason to change** — if a name needs "and", it is two units; if two files can't change independently, they may be one concept wrongly split.
- **Split at concept boundaries, on evidence** — never on a size counter. A 500-line coherent state machine beats three entangled 170-line files.
- **The newcomer test** — a newcomer should predict from names alone where a behavior lives before opening the file.
- **No junk drawers** — modules named `utils`, `helpers`, `misc`, or `manager` are banned; shared behavior earns a named home or stays where it is used.
- **Vertical slices in the apps** — features (browse, playback, sources, search, settings) own their code; the only horizontal layers are the design system, the core binding adapter, and a tiny platform-common layer.
- **Depend downward, never sideways; compose at the edge; earn abstraction** — features never import each other; wiring happens only in the composition roots; speculative indirection is rejected until a second concrete need exists.

Complexity/length lints run at *warn* and never fail CI alone — they are prompts for the review question "is this one concept?".

## Coding standards

The documents in `.augment/rules/` (`rust-dev-pro.md`, `swift-dev-pro.md`, `kotlin-dev-pro.md`) are **normative, not advisory**, for their respective layers — including their anti-pattern tables.

## Workflow

- Install the git hooks once: `prek install` (wires both the pre-commit and commit-msg shims). Run everything with `prek run --all-files`.
- Commit messages follow [Conventional Commits](https://www.conventionalcommits.org/) (hook-enforced), signed off (`git commit -s`).
- Never commit to `main` — branch and open a PR. Local hooks mirror the fast format/lint gates; full compilation, simulator/emulator smoke tests, and the REUSE lint run in CI.
- Every new file carries an SPDX header (`AGPL-3.0-or-later` for project code) per REUSE conventions.
