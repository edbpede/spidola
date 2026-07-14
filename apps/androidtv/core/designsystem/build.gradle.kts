// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

plugins {
    alias(libs.plugins.android.library)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.kotlin.compose)
}

android {
    namespace = "dev.spidola.tv.core.designsystem"
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
}

kotlin {
    jvmToolchain(21)
}

dependencies {
    implementation(platform(libs.compose.bom))
    implementation(libs.compose.foundation)
    implementation(libs.androidx.tv.material)
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.ui:ui-text")
    implementation("androidx.compose.ui:ui-graphics")

    // ImmutableList across the poster-rail composable boundary (kotlin-dev-pro Compose stability).
    implementation(libs.kotlinx.collections.immutable)

    // The lazy, disk-cached logo pipeline (TECH_SPEC §6 artwork exception). Coil's disk/memory
    // caches are capped by the app's ImageLoader; the OkHttp fetcher pulls public logo URLs only.
    implementation(libs.coil.compose)
    implementation(libs.coil.network.okhttp)
}
