<!--
SPDX-FileCopyrightText: 2026 Spidola contributors
SPDX-License-Identifier: AGPL-3.0-or-later
-->

# build-mpvkit

The pinned MPVKit build consumed by the tvOS `PlayerMPV` engine — libmpv plus an **LGPL** FFmpeg
(TECH_SPEC §12, PRD §10).

| | |
|---|---|
| Upstream | <https://github.com/mpvkit/MPVKit> |
| Pin | `0.41.0` (mpv 0.41.0, FFmpeg n8.0.1) |
| Product linked | **`MPVKit`** — the LGPL build |
| Product forbidden | `MPVKit-GPL` |
| Artifacts | 29 binary xcframeworks, checksummed in [`mpvkit.lock`](./mpvkit.lock) |
| Guard | [`verify-mpvkit-pin.sh`](./verify-mpvkit-pin.sh), run in `.github/workflows/apple.yml` |

## Why the LGPL product

MPVKit vends two products that differ only in their FFmpeg and libmpv build flags. We link the
LGPL one, and the GPL one is a release blocker.

The reason is distribution, not preference. Spidola's own code is AGPL-3.0-or-later, and the AGPL
is a *stronger* copyleft than the GPL — so mixing in GPL-2.0 libraries is not an AGPL problem, it
is an **App Store** problem. The App Store's terms impose usage restrictions the GPL forbids
adding, which is why GPL apps get pulled from it. The LGPL has no such conflict when the library
is dynamically linked and replaceable, which is exactly how these xcframeworks ship. This is the
decision recorded in TECH_SPEC §14 ("AGPL-3.0-or-later with LGPL media libraries") and PRD §10.

Concretely, the GPL build's one functional gain is Samba (`libsmbclient`) support — a protocol no
IPTV source uses. We give up nothing that matters.

`verify-mpvkit-pin.sh` enforces this rather than trusting it, because `MPVKit-GPL` is one
character away from `MPVKit` in a manifest and nothing in a build would complain: the app would
simply become undistributable, and we would find out from review, not from CI.

## On TLS: OpenSSL and GnuTLS are expected here

The LGPL `_MPVKit` target links, via `_FFmpeg`, both **OpenSSL** (`Libssl`, `Libcrypto`) and
**GnuTLS** (with `gmp`, `nettle`, `hogweed`).

This does not contradict the "no OpenSSL" rule in `.augment/rules/rust-dev-pro.md`. That rule is
scoped to the **Rust core**, where TLS is rustls with platform roots (TECH_SPEC §12) and pulling in
OpenSSL would mean a C dependency, a build-time toolchain, and a CVE feed we would own. None of
that applies to mpv's vendored TLS: it is inside a prebuilt binary we do not compile, on the media
path rather than the credential path, and removing it would mean forking MPVKit's build.

On licences, both are compatible with AGPL-3.0-or-later:

- OpenSSL 3.x is **Apache-2.0** — permissive, and no longer the old OpenSSL/SSLeay licence whose
  advertising clause was the historical GPL incompatibility.
- GnuTLS is **LGPL-2.1-or-later**; `gmp` and `nettle`/`hogweed` are LGPL-3.0-or-later (dual-licensed
  GPL-2.0-or-later).

All sit inside the dependency licence allow-list cargo-deny enforces for the core (permissive plus
LGPL, copyleft-incompatible denied — TECH_SPEC §12).

## What `mpvkit.lock` records, and what it does not

`mpvkit.lock` pins the version and the sha256 of **every binary artifact the LGPL product links** —
the transitive closure of the `_MPVKit` target, computed from MPVKit's own `Package.swift` at the
pinned tag. `Libmpv` is `9ff5077d…`; the other 28 are listed alongside it.

Two honest caveats:

1. **SwiftPM downloads more than it links.** A resolve fetches *every* `binaryTarget` in MPVKit's
   manifest, including the `-GPL` ones and `Libsmbclient`, because SwiftPM does not prune by
   product. They land in the local artifact cache and are never linked into the app, so nothing
   GPL is distributed — but "the GPL zips are on the build machine" is true, and worth knowing
   before someone reports it as a finding. The lock deliberately lists only the linked closure;
   the guard fails if a GPL artifact appears in it.
2. **`Libluajit` is macOS-only.** It is in the `_MPVKit` closure but gated to macOS by MPVKit's
   manifest, so it is not linked into the tvOS app. It is listed for completeness of the pin.

The checksums are SwiftPM's own — it verifies each archive against them on download and refuses a
mismatch, so the lock is a committed, reviewable copy of what SwiftPM already enforces. Its value
is that a change to them shows up in a diff.

## Rebuilding the LGPL binaries from source

The pin consumes MPVKit's published binaries. Rebuilding them is not required for a normal build,
and is documented here because an LGPL distribution has to be reproducible from source by whoever
receives it, and because a supply-chain review needs the flags on the record.

MPVKit builds through its own tooling. From a checkout of the pinned tag:

```bash
git clone https://github.com/mpvkit/MPVKit.git
cd MPVKit
git checkout 0.41.0

# The LGPL build. `make build` is the LGPL path; `make gpl` is the one we must not take.
make build platform=tvos,tvsimulator

# Artifacts land in ./dist/release/*.xcframework.zip
```

`make build` shells out to `swift run --package-path Sources/BuildScripts build`, which fetches
each upstream source at its pinned version and configures it. The flags that make this the LGPL
build, as recorded in `Sources/BuildScripts/XCFrameworkBuild/main.swift` at `0.41.0`:

| Component | Flag | Effect |
|---|---|---|
| FFmpeg | `--enable-gpl` **omitted** | LGPL-2.1+. `make gpl` appends it; `make build` does not. |
| libmpv | `-Dgpl=false` | LGPL build of mpv. |
| libmpv | `-Dlibmpv=true` | Build the client library rather than the CLI player. |
| libmpv | `-Dmoltenvk=enabled` | The MoltenVK GPU context (see below). |

To confirm a rebuild matches the pin, checksum the output the way SwiftPM does and compare against
`mpvkit.lock`:

```bash
swift package compute-checksum dist/release/Libmpv.xcframework.zip
# expect: 9ff5077d675a1e12bec98db167a49f46eb57dba567f40558b7758d4f12fb3ae7
```

Byte-identical output is not guaranteed — these builds are not reproducible in the strict sense
(timestamps and paths leak into the archives), so a differing checksum is not by itself evidence of
tampering. What the procedure establishes is that the pinned binaries correspond to the recorded
sources and flags.

## The MoltenVK patch, and why the engine renders the way it does

MPVKit carries `Sources/BuildScripts/patch/libmpv/0001-player-add-moltenvk-context.patch`, which
adds a `moltenvk` GPU context to libmpv (upstream mpv#7857, still unmerged). It creates a
`VkMetalSurfaceEXT` from a `CAMetalLayer` handed over through mpv's `wid` option.

That patch is why `PlayerMPV` renders with `vo=gpu-next, gpu-api=vulkan, gpu-context=moltenvk`
rather than through libmpv's render API: the render API offers only OpenGL and a software
renderer, OpenGL ES is deprecated on tvOS and mis-renders 10-bit video (mpv#7846), and software
rendering is not viable for a TV. See `MPVMetalSurface.swift`.

Upstream states plainly that Metal support "is only a patch version and does not officially
support it yet". That is the honest status of this path: it is the only Metal route available, it
is the one MPVKit's own tvOS demo uses, and it is not upstream mpv.

## Changing the pin

1. Update `exact:` in `apps/tvos/Packages/PlayerMPV/Package.swift`.
2. Re-resolve (`swift package resolve`) so `Package.resolved` moves.
3. Regenerate `mpvkit.lock` from the new tag's manifest — the closure and every checksum.
4. Run `tools/build-mpvkit/verify-mpvkit-pin.sh`.
5. Re-read the new tag's `main.swift` for the GPL flags. A version bump that quietly starts
   passing `--enable-gpl` on the LGPL path would defeat every check here, because all of them
   verify *which artifacts* we link, not how upstream compiled them.
