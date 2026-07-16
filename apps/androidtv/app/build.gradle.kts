// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.kotlin.compose)
    alias(libs.plugins.kotlin.serialization)
    alias(libs.plugins.licensee)
}

licensee {
    // Keep this policy aligned with deny.toml. Licensee walks this leaf application's complete
    // external graph and fails when metadata is missing or does not match one of these SPDX IDs.
    allow("AGPL-3.0-or-later")
    allow("Apache-2.0")
    allow("MIT")
    allow("BSD-2-Clause")
    allow("BSD-3-Clause")
    allow("ISC")
    allow("Zlib")
    allow("Unicode-3.0")
    allow("MPL-2.0")
    allow("CC0-1.0")
    allow("CDLA-Permissive-2.0")
    allow("LGPL-2.1-only")
    allow("LGPL-2.1-or-later")
    allow("LGPL-3.0-only")
    allow("LGPL-3.0-or-later")

    // Licensee's resolved report is packaged with the APK so the About surface can render the
    // exact graph that produced the build instead of carrying a hand-maintained dependency list.
    bundleAndroidAsset = true
    androidAssetReportPath = "licenses/android-dependencies.json"
}

val releaseStoreFile = providers.environmentVariable("SPIDOLA_ANDROID_KEYSTORE").orNull
val releaseStorePassword = providers.environmentVariable("SPIDOLA_ANDROID_STORE_PASSWORD").orNull
val releaseKeyAlias = providers.environmentVariable("SPIDOLA_ANDROID_KEY_ALIAS").orNull
val releaseKeyPassword = providers.environmentVariable("SPIDOLA_ANDROID_KEY_PASSWORD").orNull
val releaseSigningValues =
    listOf(releaseStoreFile, releaseStorePassword, releaseKeyAlias, releaseKeyPassword)
val releaseSigningConfigured = releaseSigningValues.all { !it.isNullOrBlank() }
require(releaseSigningValues.none { !it.isNullOrBlank() } || releaseSigningConfigured) {
    "Release signing is partially configured. Set all four SPIDOLA_ANDROID_* signing variables."
}

val configuredVersionCode =
    providers.gradleProperty("spidolaVersionCode").orNull?.toIntOrNull() ?: 1
require(configuredVersionCode > 0) { "spidolaVersionCode must be a positive integer." }
val configuredVersionName = providers.gradleProperty("spidolaVersionName").orNull ?: "0.0.0"

android {
    namespace = "dev.spidola.tv"
    compileSdk = libs.versions.compileSdk.get().toInt()

    defaultConfig {
        applicationId = "dev.spidola.tv"
        minSdk = libs.versions.minSdk.get().toInt()
        targetSdk = libs.versions.targetSdk.get().toInt()
        versionCode = configuredVersionCode
        versionName = configuredVersionName
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        testInstrumentationRunnerArguments["clearPackageData"] = "true"

        ndk {
            abiFilters += setOf("arm64-v8a", "armeabi-v7a", "x86_64")
        }
    }

    signingConfigs {
        if (releaseSigningConfigured) {
            create("release") {
                storeFile = file(requireNotNull(releaseStoreFile))
                storePassword = requireNotNull(releaseStorePassword)
                keyAlias = requireNotNull(releaseKeyAlias)
                keyPassword = requireNotNull(releaseKeyPassword)
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            if (releaseSigningConfigured) {
                signingConfig = signingConfigs.getByName("release")
            }
        }
    }

    buildFeatures {
        compose = true
        // The diagnostics screen names this build's version; the composition root is the only
        // module that may read it (TECH_SPEC §3.1) and hands it down.
        buildConfig = true
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_21
        targetCompatibility = JavaVersion.VERSION_21
    }

    testOptions {
        execution = "ANDROIDX_TEST_ORCHESTRATOR"
    }
}

kotlin {
    jvmToolchain(21)
}

tasks.withType<Test> {
    useJUnitPlatform()
}

dependencies {
    implementation(project(":core:corekit"))
    implementation(project(":core:designsystem"))
    implementation(project(":core:player-contract"))
    implementation(project(":feature:browse"))
    implementation(project(":feature:playback"))
    implementation(project(":feature:search"))
    implementation(project(":feature:settings"))
    implementation(project(":feature:sources"))

    // The composition root is the only module that may name engines (TECH_SPEC §3.1): it builds the
    // EngineRegistry the playback slice resolves against, which is what keeps that slice — and every
    // other feature — free of a decoder dependency.
    implementation(project(":player:engine-exo"))
    implementation(project(":player:engine-mpv"))

    implementation(platform(libs.compose.bom))
    implementation(libs.compose.foundation)
    implementation(libs.androidx.tv.material)
    implementation("androidx.compose.ui:ui")
    implementation("androidx.activity:activity-compose:${libs.versions.activity.get()}")
    implementation(libs.androidx.lifecycle.runtime)
    implementation("androidx.lifecycle:lifecycle-runtime-compose:${libs.versions.lifecycle.get()}")

    implementation(libs.navigation3.runtime)
    implementation("androidx.navigation3:navigation3-ui:${libs.versions.navigation3.get()}")

    implementation(libs.kotlinx.serialization.json)
    implementation(libs.kotlinx.coroutines)

    testImplementation(kotlin("test"))
    testImplementation(libs.junit5.api)
    testRuntimeOnly(libs.junit5.engine)

    androidTestImplementation(platform(libs.compose.bom))
    androidTestImplementation("androidx.compose.ui:ui-test-junit4")
    androidTestImplementation(libs.androidx.test.ext.junit)
    androidTestImplementation(libs.androidx.test.runner)
    androidTestUtil(libs.androidx.test.orchestrator)
    debugImplementation("androidx.compose.ui:ui-test-manifest")
}
