<!--
SPDX-FileCopyrightText: 2026 Spidola contributors
SPDX-License-Identifier: AGPL-3.0-or-later
-->

# build-libmpv-android

Builds **LGPL libmpv** for Android from pinned, checksummed sources — one `libmpv.so` plus the
matching NDK `libc++_shared.so` runtime per ABI (`arm64-v8a`, `armeabi-v7a`, `x86_64`), consumed by `apps/androidtv/player/engine-mpv`
(TECH_SPEC §7, §12; PRD §10).

Everything here is vendored and pinned by us rather than fetched blind at build time. The
approach follows the [mpv-android](https://github.com/mpv-android/mpv-android) buildscripts
lineage; the pins, the licence flags, and the verification are ours.

## Quick start

```sh
export ANDROID_HOME=/path/to/android-sdk    # or ANDROID_NDK_HOME
./build.sh                                  # all three ABIs (~1h)
./build.sh arm64-v8a                        # one ABI, while iterating
./verify-pins.sh                            # fast, offline: pins + LGPL gate
./verify-pins.sh --fetch                    # full drift check (downloads everything)
```

## Host prerequisites

The NDK is **28.2.13676358** (pinned by `docs/toolchains.md`, asserted by `toolchain.sh` from
`source.properties` — the directory name is only a convention and can lie).

```sh
brew install meson ninja nasm cmake pkg-config autoconf automake libtool
```

| Tool | Needed by |
|---|---|
| meson + ninja | freetype, harfbuzz, fribidi, libass, libplacebo, mpv |
| cmake | mbedtls (and the NDK's own cmake toolchain file) |
| nasm | libass' x86_64 assembly |
| pkg-config | every component, to find the ones built before it |
| python3 | libplacebo's glad loader generation (via its pinned jinja) |

On Apple silicon the NDK's prebuilt host toolchain is `darwin-x86_64` and runs under Rosetta.
`toolchain.sh` resolves the host tag by what exists rather than by `uname`, so this needs no
configuration.

## Why LGPL, and how it is enforced

PRD §10 and TECH_SPEC §12 require the bundled media stack to be **LGPL, never GPL, never
nonfree**. GPL components would break both the AGPL-compatibility posture and the App Store
posture the project committed to, and are a licence term the project has no authority to
impose on FFmpeg's or mpv's authors' behalf.

| Component | Flag | Result |
|---|---|---|
| FFmpeg | `--disable-gpl --disable-nonfree --enable-version3 --disable-postproc` | LGPL version 3 or later |
| mpv | `-Dgpl=false` | LGPL version 2.1 or later (`HAVE_GPL 0`) |

Three things about that table are easy to get wrong:

**mpv's flag is `-Dgpl=false`, not `--enable-lgpl`.** `--enable-lgpl` was the waf-era flag;
mpv deleted waf in 0.36 when it moved to meson. mpv declares itself `license: ['GPL2+',
'LGPL2.1+']` and `-Dgpl=false` selects the LGPL half, which turns every GPL-only component
(cdda, dvbin, dvdnav) into a hard configure error rather than a silent inclusion.

**FFmpeg's `--disable-gpl`/`--disable-nonfree` are already the defaults.** They are passed
explicitly anyway, because an explicit flag is greppable and `verify-pins.sh` greps for exactly
this. A silent default is not an assertion — it is a coincidence a future edit can revoke
without leaving a trace in review.

**FFmpeg lands on LGPL v3, not v2.1, and that is forced by the TLS provider.** Mbed-TLS is
Apache-2.0; Apache-2.0 is incompatible with LGPL v2.1 but compatible with LGPL v3, so FFmpeg
lists mbedtls in `EXTERNAL_LIBRARY_VERSION3_LIST` and refuses to configure without
`--enable-version3`. OpenSSL 3 is Apache-2.0 and hits the same rule. The only mainstream TLS
provider outside that list is GnuTLS, which needs nettle and GMP — and GMP is LGPL v3 itself,
so that route arrives at v3 anyway after three more cross-compiled components. LGPL v3 is
still LGPL, still not GPL, and still compatible with the project's AGPL-3.0-or-later code.
The tvOS stack lands in the same place — the MPVKit closure under `tools/build-mpvkit`
carries OpenSSL 3 (Apache-2.0) and GnuTLS with GMP (LGPL v3) — and the LGPLv3-versus-App-Store
question that makes v2.1 attractive is **decided in PRD §10**: v3 is accepted on both
platforms, with the reasoning (and the worst-case fallback) recorded there.

### The verification is on the output, not the flags

Flags state intent; generated headers state fact. Both component scripts assert the fact and
fail the build otherwise:

- `ffmpeg.sh` requires `#define FFMPEG_LICENSE "LGPL version 3 or later"` in FFmpeg's
  `config.h` (configure resolves the licence and emits it there).
- `mpv.sh` requires `#define HAVE_GPL 0` in mpv's `config.h`.

This catches what grepping the input cannot: a flag we forgot, a flag upstream renamed, or a
dependency that silently forced the licence up.

`verify-pins.sh` then re-checks all of it, and has been tested against a deliberately broken
tree — injecting `--enable-gpl` and corrupting a digest each fail it.

## Pins

Every source, its version, and its digest live in [`sources.lock`](./sources.lock), the only
place versions are declared. Two pin kinds:

- **tarball** — sha256 of an upstream-published release artifact. Used wherever upstream
  uploads one, because those bytes are stable.
- **git** — the commit SHA a tag resolves to. Used for mpv (which publishes no source tarball
  for v0.41.0 — its release assets are Windows/macOS binaries) and libplacebo (which needs
  submodules). A commit SHA is a content hash over the whole tree, so it is a *stronger* pin
  than a sha256 of an archive — and unlike GitHub's auto-generated tarballs, whose bytes have
  changed under upstream's feet before, it cannot drift.

Headline versions: mpv **v0.41.0**, FFmpeg **7.1.5**, libplacebo **v7.360.1**, libass
**0.17.5**, freetype **2.14.3**, harfbuzz **14.2.1**, fribidi **1.0.16**, Mbed-TLS **3.6.7**.

Two of those are not obvious:

- **libplacebo is not optional.** mpv 0.41.0 declares it a hard dependency at `>= 6.338.2` and
  sets `features['libplacebo'] = true` unconditionally. `vo=gpu` does not exist without it.
- **FFmpeg is 7.1.5, not 8.x.** mpv 0.41.0 needs libavcodec `>= 60.31.102`, which 7.1 satisfies
  with margin, and 7.1 is the branch with the longest real-world soak against this mpv release.

### Digest provenance, honestly

Mbed-TLS publishes a `sha256sum.txt`, and our pin matches it. FFmpeg and FreeType publish GPG
signatures (`.asc`) but no checksum file; libass, harfbuzz and fribidi publish neither. So for
those, the digests in `sources.lock` were derived by downloading each artifact once and hashing
it — trust-on-first-use. That pins them against **later** tampering, retagging, or a
compromised mirror, which is what the lockfile is for, but it is not the same as having
verified upstream's signature at pin time. Checking the `.asc` signatures against the
maintainers' keys would close that gap, and is a known, deliberate omission.

## What gets built, and how

```
mbedtls ──────────────────► ffmpeg ─┐
freetype ─► harfbuzz ─► libass ─────┼─► mpv ─► dist/<abi>/libmpv.so
fribidi ──────────────► libass      │
libplacebo ─────────────────────────┘
```

One script per component under `components/`, run in that order by `build.sh`.

### One media-stack .so, plus the NDK C++ runtime

Every media dependency is built as a **static** library and linked into a single **shared**
`libmpv.so`. Several of those projects contain C++, so the resulting object also needs the pinned
NDK's `libc++_shared.so`. `build.sh` stages that exact runtime beside `libmpv.so`, includes it in
the checksum manifest, and the Kotlin loader loads it before the JNI shim. The remaining dynamic
dependencies are Android system libraries, verifiable with `readelf -d`.

This is LGPL-clean, and the reasoning matters: `libmpv.so` as a whole is an LGPL work (mpv
LGPL, FFmpeg LGPL, libass ISC, FreeType FTL, HarfBuzz MIT, FriBidi LGPL, libplacebo LGPL,
Mbed-TLS Apache-2.0). LGPL's relinking requirement governs the boundary between the LGPL
library and the differently-licensed application — and that boundary here is a shared object
the user can replace, built by committed scripts from pinned sources, inside an app whose own
code is AGPL and therefore source-available anyway.

### Notable build decisions

- **FreeType is built once, without HarfBuzz**, breaking the FreeType↔HarfBuzz cycle that
  upstream resolves with a two-pass build. The cost is precise: FreeType's auto-hinter loses
  HarfBuzz-assisted coverage analysis for complex scripts. It does **not** affect shaping —
  libass links HarfBuzz directly and does its own. If complex-script subtitle hinting is ever
  reported as inadequate, the two-pass build is the fix and it belongs in `freetype.sh`.
- **libass needs `-Drequire-system-font-provider=false`**, which defaults to *true*. Its system
  font providers are fontconfig, CoreText and DirectWrite; Android has none, so the default
  fails configure outright. The consequence is real: libass cannot resolve a font by name from
  the OS, so the engine must point mpv at a font directory. Until it does, embedded fonts
  (which most ASS subtitles carry) still render.
- **libass' `checkasm`/`compare`/`profile`/`fuzz` are disabled**, and this is load-bearing
  rather than trimming. They are *executables*; meson builds executables position-independent
  by default; the Nasm language has no notion of PIE; so on x86_64 — the only ABI where libass'
  assembly is nasm — configure dies with *"Language Nasm does not support position-independent
  executable"* before compiling a line.
- **libass assembly is disabled on `armeabi-v7a` only**, because libass ships none for 32-bit
  ARM and meson's `enabled` turns that into a hard error. It stays `enabled` elsewhere so a
  missing nasm fails loudly instead of silently producing a slower libass.
- **libplacebo builds GL-only** (`-Dvulkan=disabled -Dglslang=disabled -Dshaderc=disabled`).
  The GL backend emits GLSL for the driver, so the SPIR-V compilers are dead weight — shaderc
  alone would roughly double this build. Only the four submodules the GL build reads are
  pinned; `Vulkan-Headers` and `nuklear` are not fetched.
- **No `sys_root` in the meson cross file.** Meson turns it into `PKG_CONFIG_SYSROOT_DIR`,
  which makes pkg-config prepend the sysroot to every `-I`/`-L` — *including* those for the
  libraries we just installed into our own prefix, yielding paths like
  `<ndk-sysroot>/<our-prefix>/include/freetype2` that do not exist.
- **`-Wl,-z,max-page-size=16384`** everywhere: Android 15+ refuses to load a shared library
  that is not 16 KB-aligned.

## Where the output goes

```
dist/
├── <abi>/libmpv.so        # stripped media stack, per ABI
├── <abi>/libc++_shared.so # matching pinned-NDK runtime, per ABI
├── include/mpv/*.h        # client API headers (ABI-independent)
└── checksums.sha256       # manifest of the built artifacts
```

`sources.lock` pins the **inputs**; `checksums.sha256` records the **outputs**, so a rebuilt
`.so` can be told from a substituted one.

**The built `.so` files are not committed** — deliberately. Per ABI they are tens of megabytes of
third-party LGPL binary; committing them would bloat every clone, put binaries under REUSE
annotations that inline SPDX headers cannot reach, and decouple the shipped binary from the
pins that are supposed to describe it. The repository-root `.gitignore` already ignores
`build/` everywhere; this directory's own `.gitignore` adds `downloads/`, `src/` and `dist/`.
`REUSE.toml` needs no entry for them precisely because they are never committed.

Gradle consumes `dist/` directly: `apps/androidtv/player/engine-mpv/build.gradle.kts` detects
it, wires `jniLibs` and the CMake shim, and — when it is absent — builds Kotlin only and warns.
That keeps lint and the JVM unit tests runnable on a machine that has never run this build,
without the engine ever *pretending* to work: `System.loadLibrary` fails and `MpvEngine.load()`
reports an honest terminal `EngineError`.

## Build times

Roughly 20 minutes per ABI on an M-series Mac, dominated by FFmpeg. Use `build.sh <abi>` while
iterating. Component build trees are incremental, so a re-run after a failure resumes rather
than restarting.
