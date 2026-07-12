// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

pluginManagement {
    repositories {
        google {
            content {
                includeGroupByRegex("com\\.android.*")
                includeGroupByRegex("androidx.*")
                includeGroupByRegex("com\\.google.*")
            }
        }
        mavenCentral()
        gradlePluginPortal()
    }
}
dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        mavenCentral()
    }
}

rootProject.name = "Spidola"

include(":app")
include(":core:corekit")
include(":core:designsystem")
include(":core:player-contract")
include(":player:engine-exo")
include(":player:engine-mpv")
include(":feature:browse")
include(":feature:playback")
include(":feature:sources")
include(":feature:search")
include(":feature:settings")
