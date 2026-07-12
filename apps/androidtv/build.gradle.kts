// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

plugins {
    alias(libs.plugins.android.application) apply false
    alias(libs.plugins.android.library) apply false
    alias(libs.plugins.kotlin.android) apply false
    alias(libs.plugins.kotlin.compose) apply false
    alias(libs.plugins.kotlin.serialization) apply false
    alias(libs.plugins.ksp) apply false
    alias(libs.plugins.hilt) apply false
    alias(libs.plugins.ktlint) apply false
    alias(libs.plugins.detekt) apply false
}

// Toolchain assertion (TECH_SPEC §9: "build scripts assert them"). The pinned JDK is 21;
// newer JDKs are accepted so contributors are not blocked by a point bump.
val minJdk = 21
require(JavaVersion.current().majorVersion.toInt() >= minJdk) {
    "Spidola's Android build requires JDK $minJdk or newer (found ${JavaVersion.current()})."
}

// ktlint + detekt (with the Compose ruleset) run on every module — the fast local mirror
// of the Android CI lane. Complexity/length rules stay advisory (modularity doctrine, §3.1).
subprojects {
    apply(plugin = "org.jlleitschuh.gradle.ktlint")
    apply(plugin = "io.gitlab.arturbosch.detekt")

    extensions.configure<io.gitlab.arturbosch.detekt.extensions.DetektExtension> {
        config.setFrom(rootProject.file("config/detekt/detekt.yml"))
        buildUponDefaultConfig = true
    }
    dependencies {
        add("detektPlugins", "io.nlopez.compose.rules:detekt:0.4.22")
    }
}
