// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import io.gitlab.arturbosch.detekt.Detekt

plugins {
    alias(libs.plugins.android.library)
    alias(libs.plugins.kotlin.android)
}

android {
    namespace = "dev.spidola.tv.core.corekit"
    compileSdk = libs.versions.compileSdk.get().toInt()
    defaultConfig {
        minSdk = libs.versions.minSdk.get().toInt()
    }

    // The UniFFI Kotlin bindings are a generated build artifact (TECH_SPEC §5) committed under
    // `generated/` and compiled — never hand-edited — as part of this module.
    sourceSets["main"].java.srcDir("generated")

    // `cargo run -p xtask -- package-android` generates this tree at the repository root. Keep
    // native binaries out of tracked sources while packaging them with the CoreKit AAR/APK.
    sourceSets["main"].jniLibs.srcDir(rootProject.file("../../target/jniLibs"))

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_21
        targetCompatibility = JavaVersion.VERSION_21
    }
}

kotlin {
    jvmToolchain(21)
}

dependencies {
    // The generated bindings load the core through JNA and surface async methods as suspend
    // functions over kotlinx-coroutines (TECH_SPEC §5).
    implementation("net.java.dev.jna:jna:${libs.versions.jna.get()}@aar")
    implementation(libs.kotlinx.coroutines)

    testImplementation(libs.junit5.api)
    testRuntimeOnly(libs.junit5.engine)
    testImplementation(libs.kotlinx.coroutines.test)
}

tasks.withType<Test> {
    useJUnitPlatform()
}

// The generated UniFFI bindings are a build artifact, not hand-written source: they are
// compiled but exempt from the hand-written-code linters (TECH_SPEC §5).
ktlint {
    filter {
        exclude { element -> element.file.path.contains("/generated/") }
    }
}

tasks.withType<Detekt>().configureEach {
    exclude("**/generated/**")
}
