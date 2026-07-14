# SPDX-FileCopyrightText: 2026 Spidola contributors
# SPDX-License-Identifier: AGPL-3.0-or-later
#
# Builds libspidola_mpv.so — the JNI shim — against the prebuilt LGPL libmpv produced by
# tools/build-libmpv-android/build.sh.
#
# This does NOT build libmpv. libmpv is pinned, checksummed, and licence-asserted by that
# script; rebuilding it from Gradle would put an hour of native compilation on the critical
# path of every Kotlin edit, and would quietly bypass the pins.

cmake_minimum_required(VERSION 3.22.1)
project(spidola_mpv C)

# Where build.sh put things, per ABI. LIBMPV_DIST is passed by build.gradle.kts.
set(LIBMPV_SO "${LIBMPV_DIST}/${ANDROID_ABI}/libmpv.so")
set(LIBMPV_INCLUDE "${LIBMPV_HEADERS}")

if(NOT EXISTS "${LIBMPV_SO}")
  message(FATAL_ERROR
    "libmpv.so for ${ANDROID_ABI} not found at ${LIBMPV_SO}.\n"
    "Build it first:  tools/build-libmpv-android/build.sh ${ANDROID_ABI}")
endif()

# IMPORTED rather than compiled: this is the artifact of the pinned build, consumed as-is.
add_library(mpv SHARED IMPORTED)
set_target_properties(mpv PROPERTIES IMPORTED_LOCATION "${LIBMPV_SO}")

add_library(spidola_mpv SHARED mpv_jni.c)

target_include_directories(spidola_mpv PRIVATE "${LIBMPV_INCLUDE}")

target_link_libraries(spidola_mpv
  mpv
  # ANativeWindow_fromSurface, for the Surface mpv renders into.
  android
  log)

# Android 15+ refuses to load a shared library that is not 16 KB-aligned. libmpv.so itself is
# built with the same flag by tools/build-libmpv-android/toolchain.sh; the shim needs it too,
# or the pair is only half loadable.
target_link_options(spidola_mpv PRIVATE "-Wl,-z,max-page-size=16384")
