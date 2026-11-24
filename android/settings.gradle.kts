// Maven Central through Google's mirror first: Central answers CI runners with 429 and Gradle doesn't retry that.
// HITOP for Android (android/GOAL.md). Modules: core = the app's logic in plain Kotlin (JVM, unit-
// tested on the host with a host build of hitop-ffi), app = Compose UI and Android platform code.
pluginManagement {
    repositories {
        google()
        maven("https://maven-central.storage-download.googleapis.com/maven2/")
        mavenCentral()
        gradlePluginPortal()
    }
}
dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        maven("https://maven-central.storage-download.googleapis.com/maven2/")
        mavenCentral()
    }
}
rootProject.name = "hitop-android"
include(":core", ":app")
