// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

plugins {
    alias(libs.plugins.android.library)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.kotlin.compose)
}

android {
    namespace = "dev.spidola.tv.player.engineexo"
    compileSdk = libs.versions.compileSdk.get().toInt()
    defaultConfig {
        minSdk = libs.versions.minSdk.get().toInt()
    }
    buildFeatures {
        compose = true
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_21
        targetCompatibility = JavaVersion.VERSION_21
    }

    testOptions {
        // The error mapping and the engine log through android.util.Log; return stub defaults so
        // the JVM unit tests exercise the real code path instead of the "not mocked" exception.
        unitTests.isReturnDefaultValues = true
    }
}

kotlin {
    jvmToolchain(21)
}

dependencies {
    // api: this module's whole public surface is the contract's types — a consumer holding an
    // ExoEngine holds a PlaybackEngine and matches on PlaybackState.
    api(project(":core:player-contract"))

    implementation(libs.media3.exoplayer)
    // The demuxer set the format taxonomy names (TECH_SPEC §7): HLS and DASH need their own
    // media sources; TS and progressive resolve through the default extractors in media3-exoplayer.
    implementation(libs.media3.exoplayer.hls)
    implementation(libs.media3.exoplayer.dash)
    implementation(libs.media3.ui.compose)
    implementation(libs.media3.session)

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
