plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.compose)
    alias(libs.plugins.kotlin.serialization)
    alias(libs.plugins.licensee)
}

// Shipped dependencies must be permissive (android/GOAL.md "Licences"); anything else fails the
// build. Exceptions are listed one by one with their reason.
licensee {
    allow("Apache-2.0")
    allow("MIT")
    allow("BSD-2-Clause")
    allow("BSD-3-Clause")
    allow("ISC")
    allow("Zlib")
    allow("MPL-2.0")
}

android {
    namespace = "tech.rws8.hitop"
    // Android 17 (API 37); Compose 1.13 needs the 37.1+ platform.
    compileSdk {
        version = release(37) { minorApiLevel = 2 }
    }

    defaultConfig {
        applicationId = "tech.rws8.hitop"
        minSdk = 31
        targetSdk = 37
        versionCode = 1
        versionName = "1.0.0"
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        // Screenshots and other files from UI tests land in
        // app/build/outputs/connected_android_test_additional_output (CI artifact "screenshots").
        testInstrumentationRunnerArguments["useTestStorageService"] = "true"
        ndk { abiFilters += listOf("arm64-v8a", "x86_64") }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
        }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    buildFeatures { compose = true }
    // libhitop_ffi.so per ABI from scripts/build-ffi.sh android.
    sourceSets["main"].jniLibs.directories.add(rootProject.layout.projectDirectory.dir("build/ffi/jniLibs").asFile.path)
    packaging { jniLibs { useLegacyPackaging = false } }
    // The UI tests in src/sharedTest run on the emulators (androidTest) and on the JVM under
    // Robolectric (test), with a host build of hitop-ffi: fast local runs, same tests.
    sourceSets["test"].kotlin.directories.add("src/sharedTest/kotlin")
    sourceSets["androidTest"].kotlin.directories.add("src/sharedTest/kotlin")
    testOptions {
        animationsDisabled = true
        unitTests {
            isIncludeAndroidResources = true
            all { test ->
                test.systemProperty("java.library.path", rootProject.layout.projectDirectory.dir("build/ffi/host").asFile.absolutePath)
                test.systemProperty("robolectric.graphicsMode", "NATIVE")
                test.maxHeapSize = "3g"
            }
        }
    }
}

kotlin { jvmToolchain(17) }

dependencies {
    implementation(project(":core"))
    implementation(platform(libs.compose.bom))
    implementation(libs.compose.ui)
    implementation(libs.compose.material3)
    implementation(libs.compose.ui.tooling.preview)
    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.activity.compose)
    implementation(libs.androidx.lifecycle.runtime.compose)
    implementation(libs.androidx.lifecycle.viewmodel.compose)
    implementation(libs.kotlinx.serialization.json)
    debugImplementation(libs.compose.ui.tooling)
    debugImplementation(libs.compose.ui.test.manifest)

    testImplementation(libs.junit)
    testImplementation(libs.robolectric)
    testImplementation(platform(libs.compose.bom))
    testImplementation(libs.compose.ui.test.junit4)
    testImplementation(libs.androidx.test.ext.junit)
    testImplementation(libs.androidx.test.core)
    testImplementation(libs.androidx.test.storage)
    androidTestImplementation(platform(libs.compose.bom))
    androidTestImplementation(libs.compose.ui.test.junit4)
    androidTestImplementation(libs.androidx.test.ext.junit)
    androidTestImplementation(libs.androidx.test.runner)
    androidTestImplementation(libs.androidx.test.core)
    androidTestImplementation(libs.androidx.test.storage)
    androidTestImplementation(libs.androidx.test.uiautomator)
    androidTestUtil(libs.androidx.test.services)
}
