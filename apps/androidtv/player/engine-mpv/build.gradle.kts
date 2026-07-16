// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

plugins {
    alias(libs.plugins.android.library)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.kotlin.compose)
}

// The output of tools/build-libmpv-android/build.sh: dist/<abi>/libmpv.so + dist/include.
// Not committed — tens of megabytes of third-party LGPL binary per ABI; see that directory's
// README and .gitignore.
val libmpvDist = rootProject.file("../../tools/build-libmpv-android/dist")
val libmpvHeaders = File(libmpvDist, "include")

val builtAbis =
    libmpvDist.listFiles().orEmpty()
        .filter { File(it, "libmpv.so").isFile }
        .map { it.name }
        .sorted()

// Whether the pinned libmpv artifact is present.
//
// The JNI shim links libmpv.so, so without it CMake cannot link and the whole module fails to
// assemble — taking ktlint, detekt and every JVM unit test down with it, none of which need a
// decoder. Gating the native build keeps the Kotlin and its tests verifiable on a machine that
// has never run the hour-long native build, which is also what lets the CI lint/test lane
// cover this module without it.
//
// This is not a silent degrade. With the artifact absent the .so is simply not in the APK,
// System.loadLibrary fails, and MpvEngine.load() reports an honest terminal EngineError
// instead of pretending to play. The warning exists so nobody first learns that from a device.
val hasLibmpv = libmpvHeaders.isDirectory && builtAbis.isNotEmpty()

if (!hasLibmpv) {
    logger.warn(
        "player:engine-mpv: no prebuilt libmpv at $libmpvDist — building Kotlin only. " +
            "The mpv engine will report itself unavailable at runtime. " +
            "Run tools/build-libmpv-android/build.sh to produce it.",
    )
}

android {
    namespace = "dev.spidola.tv.player.enginempv"
    compileSdk = libs.versions.compileSdk.get().toInt()

    defaultConfig {
        minSdk = libs.versions.minSdk.get().toInt()
        // Ships with the AAR so any consumer that minifies inherits the JNI keep rules; the
        // shim resolves Kotlin by name at runtime, which R8 cannot see.
        consumerProguardFiles("consumer-rules.pro")

        if (hasLibmpv) {
            externalNativeBuild {
                cmake {
                    arguments +=
                        listOf(
                            "-DLIBMPV_DIST=${libmpvDist.absolutePath}",
                            "-DLIBMPV_HEADERS=${libmpvHeaders.absolutePath}",
                            // The shim is C. libmpv's C++ runtime is staged beside it and loaded
                            // explicitly before the shim, so CMake must not add a second copy.
                            "-DANDROID_STL=none",
                        )
                }
            }
            ndk {
                // Only the ABIs the pinned build actually produced. Requesting one it did not
                // is a CMake hard error by design (cpp/CMakeLists.txt) rather than an APK
                // silently missing its player on one architecture.
                abiFilters += builtAbis
            }
        }
    }

    if (hasLibmpv) {
        externalNativeBuild {
            cmake {
                path = file("src/main/cpp/CMakeLists.txt")
                version = "3.22.1"
            }
        }
        // libmpv.so is IMPORTED by CMake, not built by it, so it is not packaged automatically.
        // Without this the shim would load against a library that is not in the APK.
        sourceSets["main"].jniLibs.srcDir(libmpvDist)
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_21
        targetCompatibility = JavaVersion.VERSION_21
    }

    buildFeatures {
        compose = true
    }

    testOptions {
        // The mapping code and the engine log through android.util.Log; return stub defaults so
        // the JVM unit tests exercise the real code path instead of the "not mocked" exception.
        // Mirrors player:engine-exo.
        unitTests.isReturnDefaultValues = true
    }

    packaging {
        jniLibs {
            // libmpv.so arrives already stripped from the pinned build. Letting AGP strip it
            // again would mean the shipped bytes no longer match dist/checksums.sha256, which
            // is the whole point of emitting that manifest.
            keepDebugSymbols += "**/libmpv.so"
        }
    }
}

kotlin {
    jvmToolchain(21)
}

dependencies {
    // api: this module's whole public surface is the contract's types — a consumer holding an
    // MpvEngine holds a PlaybackEngine and matches on PlaybackState. Mirrors engine-exo.
    api(project(":core:player-contract"))

    implementation(platform(libs.compose.bom))
    implementation(libs.compose.foundation)
    implementation("androidx.compose.ui:ui")

    implementation(libs.kotlinx.collections.immutable)
    implementation(libs.kotlinx.coroutines)

    testImplementation(kotlin("test"))
    testImplementation(libs.junit5.api)
    testRuntimeOnly(libs.junit5.engine)
    testImplementation(libs.mockk)
}

tasks.withType<Test> {
    useJUnitPlatform()
}
