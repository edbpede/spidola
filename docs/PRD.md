# Spidola — Product Requirements Document

| | |
|---|---|
| **Name** | Spidola |
| **Document status** | Draft v1.1 — July 2026 |
| **License** | AGPL-3.0-or-later |
| **Companion documents** | `TECH_SPEC.md` — architecture, stack, engineering standards · `IMPLEMENTATION_PLAN.md` — phased task breakdown |
| **Platforms** | Apple TV (tvOS) and Android TV / Google TV |

---

## 1. Vision

Spidola is a free, open-source IPTV player built exclusively for the living room. Where most IPTV apps on TV platforms are ports of phone apps — cluttered, ad-riddled, slow, and hostile to remote-control navigation — Spidola is designed from the first pixel for the 10-foot experience: a D-pad, a couch, and a person who wants to be watching television within seconds of pressing the home button.

The project's founding principles: user-supplied sources, no bundled content, obsessive speed, instant search, low memory, and zero monetization pressure inside the app — delivered natively on the two dominant TV platforms, with the remote control treated as the primary, not adapted, input device.

The one-sentence pitch: **the fastest, cleanest way to watch your own IPTV sources on the television you already own.**

## 2. Background and prior art

Desktop IPTV players have proven a recipe worth keeping: a fast native engine handling playlist ingestion, an SQLite channel database with instant search, and mpv-class playback for codec breadth — typically with mpv launched as an external process. That last part cannot be transplanted to TV platforms: neither tvOS nor Android TV permits launching external player binaries, and neither runs a webview-centric desktop shell well. Spidola therefore keeps the *engine* philosophy (Rust core, SQLite, mpv-class codec breadth) but builds the *shell* natively per platform, with playback embedded as a library rather than a child process. The full architectural translation is specified in the tech spec.

## 3. Goals

The product succeeds if it delivers, in priority order: first, a playback experience that starts fast and plays the messy reality of IPTV streams (raw MPEG-TS, odd codecs, misdeclared containers) without the user needing to understand why; second, source management that swallows any common IPTV input — M3U file, M3U URL, or Xtream Codes API — including very large playlists (tens of thousands of channels) without stutter; third, navigation that a family member can operate with only a remote and no instructions; and fourth, a codebase that is a pleasure to contribute to, with a cohesive, modular, single-purpose code organization and an AGPL-3.0-or-later license guaranteeing the app and its derivatives stay free.

## 4. Non-goals

Spidola is not a content service: it ships with no channels, no playlists, no directories of sources, and no links to any. It does not scrape, index, or recommend third-party content. It is not a general media-center (no local library management, no metadata scraping à la Plex/Jellyfin — those projects already exist and do it well). It is not a phone or tablet app in v1; phones are addressed only as far as shared code makes a later port cheap. It does not include accounts, cloud sync of user data through project-operated servers, telemetry, or advertising — ever. Desktop platforms are out of scope; that space is already well served by existing open-source players.

## 5. Target users and platforms

### 5.1 Platforms

| Platform | Minimum OS | Reference hardware | Distribution |
|---|---|---|---|
| tvOS | tvOS 18.0 deployment target, built with the current tvOS 26 SDK | Apple TV 4K (2nd gen, A12) and newer; Apple TV HD best-effort | App Store; source builds |
| Android TV / Google TV | Android 8.0 (API 26) minimum, targeting current API | Chromecast with Google TV, onn. 4K, Nvidia Shield TV, Sony/TCL/Hisense Google TV sets | Google Play (TV form factor); direct APK from releases; F-Droid candidate |

Android TV hardware is heterogeneous and often weak (1–2 GB RAM boxes are common); the performance budgets in the tech spec treat the low-end Chromecast-class device, not the Shield, as the baseline.

### 5.2 Personas

**The self-hoster.** Runs their own services, has a legitimate IPTV subscription or a headend of their own, and is allergic to closed-source players with subscription unlock screens. Cares about codec support, source privacy, and the AGPL license. Will file excellent bug reports.

**The household.** One technical person set the app up; everyone else just uses it. They know four buttons: up, down, select, back. Success for this persona is never seeing an error dialog they can't act on, and finding "their" channels (favorites) within two clicks of launch.

**The channel-zapper.** Grew up with broadcast TV and wants that muscle memory back: flip channels with up/down during playback, glance at what's on now and next, and never think about "sources" or "buffers." EPG quality and the in-playback channel strip matter most to them.

## 6. Feature requirements

Priorities: **P0** is the MVP release gate, **P1** is the fast-follow (target within two release cycles of 1.0), **P2** is roadmap. Each area below is a requirement cluster; acceptance criteria live with the issue tracker, not this document, but the defining behaviors are stated here.

### 6.1 Source management — P0

The user can add a source in one of three forms: an M3U/M3U8 playlist by URL, an M3U/M3U8 playlist from a local file (tvOS: document picker / paste; Android TV: storage access framework or paste), and an Xtream Codes-compatible account (server URL, username, password). Multiple sources coexist; each is named, individually refreshable, individually deletable, and can be temporarily disabled without deletion. Refresh is manual plus an optional per-source automatic interval. Import of a 50,000-channel playlist must complete without the UI blocking and with progress reported; parsing is incremental and memory-bounded (budgets in the tech spec). Credentials for Xtream sources are stored in the platform's secure storage, never in plaintext. Because typing on a TV is miserable, every text entry field supports the platform's phone-keyboard remote-input flow, and source addition also offers a **pairing shortcut**: the TV displays a local URL and QR code, the user opens it on their phone, and pastes the playlist URL or Xtream details into a plain form served by the app on the local network only. This pairing server binds to the LAN, runs only while the add-source screen is open, and serves no user data.

### 6.2 Browsing and navigation — P0

Content is organized by the axes IPTV sources actually provide: source → type (live / movies / series, where the source distinguishes them) → category/group → channel. The home screen surfaces favorites first, then recently watched, then categories. All lists are virtualized and instant; scrolling 10,000 channels in a category must not hitch. Channel logos load lazily with graceful placeholders and an aggressive disk cache. Series (from Xtream sources) expand into seasons and episodes. A long-press (or dedicated context button) on any channel opens a context menu: play, favorite/unfavorite, hide, view details, choose player engine for this channel.

### 6.3 Playback — P0

Selecting a channel starts playback full-screen with a target of under two seconds from click to first frame on an HLS stream on reference hardware. The playback surface exposes: pause/resume where the stream allows it, an info overlay (channel name, current/next program when EPG is available, stream health), audio track selection, subtitle track selection and toggle, aspect-ratio cycling, and — the signature interaction — **channel zapping**: up/down on the D-pad during playback flips to the adjacent channel in the current list, and select summons the channel strip (§8.5) for browsing without leaving playback. Playback engines are dual per platform: on Android TV the default engine is Media3/ExoPlayer with libmpv as the fallback; on tvOS the default engine is MPVKit (libmpv), with AVPlayer available as an alternative engine for HLS-native streams. Engine choice is automatic-with-override: a global default, a per-source override, and a per-channel override, plus an automatic fallback prompt when the default engine fails with a decode/container error (policy detailed in the tech spec). Errors are always actionable: a failed stream names the failure class in plain words (unreachable, unauthorized, unsupported format) and offers retry, try the other engine, or go back.

### 6.4 Search — P0

Search is global, instant (results update per keystroke with a sub-50 ms budget against the local database), and reachable from anywhere via a persistent affordance. It matches channel names with prefix and fuzzy tolerance, filterable by source and type. On Android TV, the app also integrates with the system-level content search where the platform allows it (P1). Voice search input is supported wherever the platform remote provides it, as plain text input into the same search path.

### 6.5 Favorites and personalization — P0

Any channel, movie, or series can be favorited; favorites form the first row of home and the default zap list during playback. Channels can be hidden (per user action, reversible in settings). Sort order within favorites is user-arrangeable (P1). "Recently watched" is maintained locally with a one-toggle purge and an off switch for the privacy-minded.

### 6.6 EPG (electronic program guide) — P1

EPG data comes from Xtream's EPG endpoints and from user-supplied XMLTV URLs mapped to sources. The guide renders as a now/next strip on channel rows (P1) and a full timeline grid (P2). EPG ingestion is background, incremental, and bounded: the app stores a rolling window (default 3 days ahead, 6 hours behind) and prunes beyond it. Program details show title, time, and description. Reminders and EPG-driven recording are out of scope for now (see recording, §6.8).

### 6.7 Custom channels and sharing — P1

Users can create custom channels (name, URL, optional logo, optional headers/user-agent) and group them. Custom channel groups can be exported to a portable file and imported on another device — this is also the interim answer to cross-device sync (true sync is an open question, §13).

### 6.8 Recording and restreaming — P2, Android TV only

Recording while watching (remuxing the input stream to local storage) is feasible on Android TV with sufficient storage and is scoped as a P2 Android-only feature. It is explicitly **not** planned for tvOS: the platform's storage model (purgeable caches, no user-visible file system, tight persistent-storage expectations) makes honest recording promises impossible. Restreaming to other devices is deferred to P2 and carries an AGPL section-13 note (§10). If it lands, it reuses the same LAN-only, on-while-visible server posture as the pairing shortcut.

### 6.9 Settings — P0

Settings cover: default player engine per platform and per source; buffering profile (low-latency vs. stable, mapped to engine parameters); subtitle appearance (size, background); UI language; interface density; recently-watched retention; EPG window; cache size and a clear-cache action; and a diagnostics screen (log level, export logs, versions of app/core/engines). Every setting has a sane default; the app must be fully usable without ever opening settings.

### 6.10 Accessibility and localization — P0 baseline, ongoing

The app respects platform screen readers (VoiceOver on tvOS, TalkBack on Android TV) on every focusable element, honors system reduce-motion settings by disabling non-essential animation, meets WCAG AA contrast on all text against its background (the palette in §8 is chosen to pass at TV viewing sizes), and keeps all type at or above platform 10-foot minimums. Localization ships English-first with the string infrastructure ready from day one; community translations are invited post-1.0 via a standard localization platform.

## 7. Platform parity policy

Feature parity between tvOS and Android TV is the default and any divergence must be justified by a platform constraint, documented in this section. Current sanctioned divergences: recording (Android only, storage model, §6.8); system content-search integration (Android only, platform capability); default engine (MPVKit on tvOS vs. ExoPlayer on Android, justified in the tech spec §8); and Top Shelf / home-screen channel promotions, which each platform implements in its own idiom (tvOS Top Shelf extension, Android TV home-screen channels/watch-next rows — both P1).

## 8. UX and design direction

### 8.1 Design thesis

The subject is *television* — the broadcast medium itself, with fifty years of visual vernacular: test cards, lower-thirds, channel numbers, the instant of the zap. The design borrows from that world deliberately and quietly, rather than dressing an app in streaming-service chrome. One risk is taken and spent in one place (§8.5, the channel strip); everything else is disciplined, dark, and typographic.

Dark-first is a considered choice, not a default: the app lives on living-room panels, frequently OLED, usually in dim rooms, and its content is full-motion video — a dark canvas reduces glare, protects perceived contrast of the video itself, and is the established convention users' eyes expect at 10 feet. The discipline is in *which* dark and what sits on it.

### 8.2 Color

The palette is five named values. **Studio** `#12151A` is the canvas — a near-black with a cool cast, dark enough for OLED rooms but lifted off pure black so panels don't smear. **Set** `#1C2129` is the raised surface for cards, rails, and overlays. **Broadcast White** `#F1EFE9` is primary text — a warm paper-white that reads softly at distance. **Static** `#8B94A3` is secondary text and inactive metadata. The single accent is **Test-Card Amber** `#E3A44A`, drawn from the yellow bar of the classic SMPTE test pattern; it marks exactly three things — focus, the live indicator, and primary actions — and appears nowhere else. Semantic red/green appear only in stream-health and error contexts, muted to sit in the same tonal family. No gradients on surfaces; the video is the color in this app.

### 8.3 Typography

Body and UI text use the platform system faces — SF Pro on tvOS and Roboto on Android TV — a deliberate concession to the medium: TV rendering stacks hint and scale these faces best, and 10-foot legibility beats brand vanity. Personality lives in the display layer: screen titles, the channel strip, and empty states are set in a characterful grotesque with a slightly extended width (Archivo, SIL OFL — license-compatible with AGPL distribution), used at few sizes, heavy weights, and generous tracking. Numerals are tabular everywhere times or channel numbers appear. The type scale is short and strict — roughly display, title, body, caption — with minimum body size at the platform's 10-foot floor and no text below caption ever focusable.

### 8.4 Layout, focus, and the remote

Every screen is designed for D-pad traversal first: focus order is predictable (left rail → content grid → context), the focused element is always unmistakable (scale plus Test-Card Amber underline/border, per platform idiom), and focus is never trapped or lost on data refresh. All content respects TV-safe margins on both platforms. The remote mapping is consistent app-wide:

| Input | Browsing | During playback |
|---|---|---|
| D-pad up/down | Move focus | Zap to previous/next channel |
| D-pad left/right | Move focus | Seek where stream allows; otherwise no-op with hint |
| Select | Open / play | Summon channel strip |
| Back | Up one level | Dismiss overlay, then stop and return |
| Play/pause | Play focused item | Pause/resume |
| Long-press select / menu | Context menu | Playback options (tracks, aspect, engine) |

### 8.5 The signature: the channel strip

The one memorable element is the in-playback channel strip — a broadcast-style lower-third that slides up over live video on select. It shows the current channel's logo, name, and now/next EPG in a single 
disciplined band, with adjacent channels peeking above and below for zap-ahead browsing; a thin ribbon of the SMPTE bar spectrum, three pixels tall, underlines the band as the only decorative flourish in the entire app. It is fast (appears in one frame, never stalls video), dismisses on back or timeout, and is the primary way the channel-zapper persona lives in the app. Every other surface stays quiet so this one can sing.

### 8.6 Motion and copy

Motion is limited to what TV platforms do natively well: focus scale, the channel strip's slide, and crossfades on content load — each under 200 ms, all suppressed under reduce-motion. Copy is written from the couch: controls say what they do ("Add source," "Try other player," "Hide channel"), errors say what happened and what to press next, and empty states are invitations ("No sources yet — add one to start watching"). No system jargon reaches the screen: users manage *sources* and *channels*, never *playlists parsed* or *FFI errors*.

## 9. Quality bars and success metrics

The project tracks no telemetry, so success is measured by public signals and local benchmarks: cold start to interactive home under 1.5 s on reference hardware; click-to-first-frame under 2 s for HLS on the default engine; search keystroke-to-results under 50 ms at 50k channels; playlist import of 50k channels under 30 s on the low-end Android baseline; zero UI hitches over 100 ms during list scroll in release builds (verified in CI where tooling allows and by profiling checklist per release); crash-free sessions above 99.5% as reported by store consoles' built-in (opt-in, OS-level) crash reporting only. Community metrics: time-to-first-response on issues under a week, and store ratings held above 4.0 once volume exists.

## 10. Licensing, legal, and store compliance

The project is licensed **AGPL-3.0-or-later** (decided; SPDX identifier `AGPL-3.0-or-later` on every file, REUSE conventions throughout the repository). Three practical consequences are accepted with eyes open. First, **App Store friction**: Apple's standard App Store terms have historically been read (by the FSF, and in the VLC/GPLv2 episode) as conflicting with strong copyleft. As the sole initial copyright holder, the project owner can lawfully distribute their own AGPL code on the App Store — one cannot infringe one's own copyright — but the moment third-party AGPL contributions are merged, contributors' licenses bind the distribution too. The project therefore requires a lightweight contributor agreement **decision before accepting external code**: either a DCO-plus-explicit App Store distribution permission, or a CLA granting the maintainer distribution rights, or acceptance that App Store releases are built only from maintainer-copyright code. This is flagged as a launch-blocking governance decision, not a footnote. Second, **the network clause**: AGPL section 13 concerns users interacting with the software over a network; the pairing server (§6.1) and any future restreaming (§6.8) must surface a "source code" link in their served pages to keep compliance trivially true. Third, **dependency compatibility**: all bundled components must be AGPL-compatible; in particular the mpv/FFmpeg builds embedded on both platforms are configured LGPL (no GPL-only FFmpeg components), which is both AGPL-compatible and the safer posture for App Store review. Engineering enforcement (license audits in CI) is specified in the tech spec.

Store-policy posture: the app ships with no content and no source directory, states this plainly in store listings, and provides reviewers a demo source (maintainer-operated, containing only self-produced or public-domain test streams) because both stores' reviewers require a working demonstration. Apple's review history with generic IPTV players is uneven; the mitigation is scrupulous content-neutrality, a working demo, and — worst case — distribution on tvOS via source builds while appealing. Google Play requires the TV form-factor review checklist (banner asset, D-pad completeness, no phone-only UI), which §8 satisfies by construction.

Privacy: no analytics, no third-party SDKs with network behavior, no account. The privacy policy (required by both stores) is one page and truthfully says data never leaves the device except to fetch the user's own sources.

## 11. Release milestones

| Milestone | Scope | Definition of done |
|---|---|---|
| **M0 — Skeleton** | Rust core workspace, FFI bindings building on both platforms, hello-world apps rendering a channel list from a fixture playlist | CI green on all targets; both apps run on real hardware |
| **M1 — Watchable** | M3U URL import, browsing, default-engine playback, favorites | A household member can watch a channel unaided |
| **M2 — MVP / 1.0** | All P0: Xtream, file import, pairing shortcut, search, dual engines with fallback, settings, accessibility baseline, store submissions | Store approvals or documented appeals in flight |
| **M3 — 1.x** | P1: EPG now/next, custom channels, Top Shelf / home-channels, favorites ordering, system search (Android) | — |
| **M4 — 2.0 track** | P2: EPG grid, recording (Android), restreaming decision, sync decision | — |

## 12. Risks

The top risks, each with its mitigation: **App Store rejection or AGPL conflict** (mitigated per §10; fallback is tvOS source-distribution while appealing — accepted as survivable because Android distribution is unconstrained); **libmpv on tvOS integration depth** (MPVKit is proven in shipping tvOS apps, but rendering-surface and audio-session edge cases are expected; mitigated by the dual-engine design — AVPlayer keeps HLS users watching even if mpv work drags); **low-end Android performance** (mitigated by the Rust core doing all heavy lifting off the UI thread and the budgets in §9 being CI-enforced where possible); **Rust-on-tvOS toolchain maturity** (recently promoted to Tier 2, which removes the worst risk, but the target remains young; the tech spec pins toolchains and keeps a build-std fallback documented); and **solo-maintainer bus factor** (mitigated by the contribution-friendly goals in §3 and the governance decision in §10 being made *before* contributors arrive).

## 13. Open questions

Two questions from earlier drafts are resolved: the original working name ("Orbita") failed the trademark / store-name availability check and was replaced by "Spidola" (ADR-0002; reserving the name in App Store Connect remains the definitive test), and the contributor agreement model is DCO plus an explicit App Store distribution exception (ADR-0001, §10). Cross-device sync: out of scope, export/import only, or a self-hostable sync target in the 2.0 era? Restreaming: build it into Spidola or leave it to dedicated tools? Whether Apple TV HD (A8) is worth the performance floor it imposes, or whether tvOS 18+/A12+ is the honest line (current draft assumes A12+).
