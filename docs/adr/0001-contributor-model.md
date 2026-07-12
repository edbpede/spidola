# ADR-0001 — Contributor model: DCO plus App Store distribution exception

| | |
|---|---|
| **Status** | Accepted |
| **Date** | 2026-07-12 |
| **Deciders** | Maintainer |
| **References** | PRD §10 (licensing, legal, store compliance) · TECH_SPEC §14 (decision-log seed) · IMPLEMENTATION_PLAN Phase 0 · Governance |

## Context

The project is AGPL-3.0-or-later and targets the Apple App Store. Apple's standard store terms have historically been read (FSF position; the 2011 VLC removal) as imposing restrictions incompatible with strong copyleft. The sole initial copyright holder can distribute their own code there, but once third-party contributions are merged, every contributor's copyright binds distribution too. PRD §10 flags this as a **launch-blocking governance decision** with three options: (a) DCO plus an explicit App Store distribution permission, (b) a CLA granting the maintainer distribution rights, or (c) App Store releases built only from maintainer-copyright code.

## Decision

Option (a): **DCO 1.1 sign-off plus a standing additional permission under AGPL §7** granting App Store distribution.

- `APPSTORE_EXCEPTION.md` (repo root) carries the additional-permission text. AGPL §7 expressly allows copyright holders to add permissions to their material and allows any recipient to remove them; the exception adds one permission for one channel and restricts nothing.
- `CONTRIBUTING.md` states that every contribution is licensed AGPL-3.0-or-later **together with** the exception, certified by the DCO sign-off (`git commit -s`).
- Enforcement: a local `prek` commit-msg hook checks for the `Signed-off-by:` trailer; the **GitHub DCO check app** on pull requests is the authoritative gate (local hooks are opt-in). The DCO app must be installed on the repository before the first external PR is merged.

A full CLA is rejected: it adds signature-database overhead and contribution friction disproportionate to a free-software project whose only special need is one narrow distribution permission, and it conflicts with the PRD §3 goal of a contribution-friendly codebase.

## Consequences

- Contributors keep their copyright; friction is one `git commit -s` flag. No signature infrastructure to run.
- App Store distribution remains lawful after external contributions, with the reasoning documented in-tree and true by construction (source always available under AGPL; the exception is additive only).
- The evidentiary weight of a DCO sign-off is lighter than a signed CLA. Accepted: the fallback remains PRD §10 option (c) — App Store builds excluding any contribution whose grant is in doubt — and Android/direct distribution is never constrained.
- Any relicensing or store-policy change revisits this ADR.
