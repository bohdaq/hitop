// HITOP for Android (android/GOAL.md). Modules: core = the app's logic in plain Kotlin (JVM, unit-
// tested on the host with a host build of hitop-ffi), app = Compose UI and Android platform code.
pluginManagement {
    repositories {
        google()
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
rootProject.name = "hitop-android"
include(":core", ":app")
