pluginManagement {
    includeBuild("build-logic")
    repositories {
        google {
            content {
                includeGroupByRegex("com\\.android.*")
                includeGroupByRegex("com\\.google.*")
                includeGroupByRegex("androidx.*")
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

rootProject.name = "teach_me"

include(":app")
include(":core:designsystem")
include(":core:model")
include(":core:network")
include(":core:data")
include(":core:ui")
include(":sdui")
include(":feature:login")
include(":feature:settings")
include(":feature:home")
include(":feature:folder")
include(":feature:mission")
include(":feature:resources")
include(":feature:chat")
include(":feature:lesson")
include(":feature:review")
