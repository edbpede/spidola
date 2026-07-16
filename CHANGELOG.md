<!--
SPDX-FileCopyrightText: 2026 Spidola contributors
SPDX-License-Identifier: AGPL-3.0-or-later
-->

# Changelog

## Unreleased

### Features

- complete Phase 6 — Xtream, pairing, settings, and the accessibility baseline (#15) (`9116867`)
- **playback:** implement Phase 5 player contract, four engines, and the channel strip (#12) (`05903ab`)
- implement Phase 4 sources, catalog, and search (toward M1) (#10) (`ee3fa62`)
- **docs:** add project branding and README (#9) (`b553705`)
- **android-tv:** complete M0 emulator verification (#7) (`712d4ec`)
- **tvos:** complete M0 core integration and simulator tests (#6) (`fef0d34`)
- **apps:** scaffold Phase 3 walking-skeleton apps (M0) (#5) (`fe8d5a2`)
- **ffi:** UniFFI boundary, packaging, and contract-test parity keel (#4) (`fced8a2`)
- **core:** implement Phase 1 Rust core foundations (#2) (`e5a61d6`)
- bootstrap repository, governance, and toolchains (Phase 0) (#1) (`a6578ff`)

### Fixes

- harden pre-phase 7 validation (#16) (`970eaf3`)
- **playback:** engine deinit backstop, zap-path kind coverage, and the LGPL-3.0 decision (PR #12 follow-ups) (#13) (`374653e`)
- delete failed-import sources on Android and stabilize the TV emulator CI (#11) (`b977167`)
- **ci:** stabilize Android TV emulator workflow (#8) (`6e097d7`)
- **ci:** repair apple and android toolchain pins (#3) (`252c3c0`)

### Documentation

- defer hardware acceptance to phase 8 and consolidate remaining work into phase 7 (`fe24bdc`)
- resolve all open questions ahead of implementation (`cf32514`)
- drop ADR references from IMPLEMENTATION_AGENT_PROMPT (`35321fe`)
- remove ADRs in favor of inline decision logging (`e074b6a`)
- rename project from Orbita to Spidola (`5d66975`)
- **governance:** adopt DCO plus App Store distribution exception (`26e9a85`)
- add reusable implementation agent prompt (`305e0bb`)
- add product, architecture, and implementation-plan docs (`9055b28`)

### Build

- **prek:** gate commit messages on DCO sign-off (`8dffb53`)
- add root .gitignore and prek pre-commit config (`6c6bd51`)

### Maintenance

- **docs:** remove agent prompt (`8705add`)

### Other

- Initial commit (`8a66b1e`)
