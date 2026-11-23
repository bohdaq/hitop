// The app's logic without Android: models, storage, HITOP Cloud, sync, runs. Unit tests run on the
// JVM with a host build of hitop-ffi (scripts/build-ffi.sh host), so they need no emulator.
plugins {
    `java-test-fixtures`
    alias(libs.plugins.kotlin.jvm)
    alias(libs.plugins.kotlin.serialization)
}

kotlin { jvmToolchain(17) }

dependencies {
    api(libs.kotlinx.serialization.json)
    api(libs.kotlinx.coroutines.core)
    testImplementation(libs.junit)
    testImplementation(libs.kotlinx.coroutines.test)
    testFixturesImplementation(libs.kotlinx.serialization.json)
}

val ffiHost = rootProject.layout.projectDirectory.dir("build/ffi/host")
tasks.withType<Test>().configureEach {
    // The host library from scripts/build-ffi.sh host (CI builds it first).
    systemProperty("java.library.path", ffiHost.asFile.absolutePath)
    environment("HITOP_REPO", rootProject.layout.projectDirectory.asFile.parentFile.absolutePath)
    // End-to-end tests read these from the environment when CI sets them.
    listOf("HITOP_SYNC_SERVER_BIN", "HITOP_BIN", "HITOP_E2E_REQUIRED", "HITOP_TEST_SERVERS_BIN").forEach { name ->
        System.getenv(name)?.let { environment(name, it) }
    }
    testLogging { events("failed"); exceptionFormat = org.gradle.api.tasks.testing.logging.TestExceptionFormat.FULL }
}
