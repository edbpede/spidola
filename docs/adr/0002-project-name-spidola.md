# ADR-0002 — Project name: Spidola (replacing the working name Orbita)

| | |
|---|---|
| **Status** | Accepted |
| **Date** | 2026-07-12 |
| **Deciders** | Maintainer |
| **References** | PRD §10 (store compliance) · PRD §13 (open questions) · IMPLEMENTATION_PLAN Phase 0 · Governance |

## Context

Phase 0 requires a trademark / store-name availability check for the working name **Orbita**. The check (web and store-listing sweep, July 2026) failed it on three grounds:

- The exact app name "Orbita" is already in use on the Apple App Store (a smart-lock app by Guangdong Orbita Technology, which also holds a 2024/25 Chinese class-9 software trademark registration) — and Apple requires app names to be unique.
- A same-niche collision: "Orbita" is an Israeli IPTV service (orb-media.com) shipping set-top-box software, plus "Orbita Digital" IPTV encoder/decoder hardware — the field where trademark-confusion analysis actually bites.
- Heavy general dilution: a UK-registered ORBITA® health app, a US healthcare-AI company (orbita.ai), GoLogin's Orbita browser, a renewed US registration (watch winders), and a dozen smaller apps.

## Decision

The project is named **Spidola**, after the **VEF Spīdola** (Riga, Latvian SSR) — the Soviet Union's first mass-produced transistor radio (1960), so ubiquitous that *spidola* became the genericized Russian word for any portable receiver. The name traces to Spīdola, the sorceress of the Latvian national epic *Lāčplēsis* (from *spīdēt*, "to shine"). An IPTV client is, precisely, a receiver.

Availability evidence at decision time:

- No app, software product, or store listing named "Spidola" found on the Apple App Store, Google Play, or the wider software market.
- The historic mark is genericized and long-lapsed; remaining namesakes are non-software (a small Riga LLC, a Latvian Navy heritage vessel, a font, a sorority).
- `spidola.app`, `spidola.tv`, `spidola.dev`, `getspidola.com` all returned NXDOMAIN.

Candidates checked and rejected for collisions: Kineskop (existing IPTV app), Šilelis, Orava, Pravetz (brands revived and active), Ekran, Efir, Luch, Raduga, Kadr, Banga, Chromat, Molniya, Strela, Vzor, Yunost, Lunokhod. Clean runners-up recorded for posterity: Rigonda, Shabolovka, Selga.

## Consequences

- All repository documents, the GitHub repository, and the working tree are renamed Orbita → Spidola.
- Remaining due diligence before 1.0 (tracked by the Phase 7 store-submission tasks): reserve the app name in App Store Connect and the Play Console (the definitive availability test), register the domains, and run a knockout search in USPTO TESS / EUIPO eSearch for live class-9/38/41 "SPIDOLA" marks in target jurisdictions.
- The project claims no trademark exclusivity; the need is freedom to use the name, consistent with the free-software posture.
