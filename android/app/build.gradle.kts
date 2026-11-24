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
    // Owner decision (android/GOAL.md "Sign-in"): Google sign-in with the native account picker
    // through Credential Manager. Its Google Play services parts and the Google ID library are
    // under the Android Software Development Kit License (not open source), shipped as Google
    // distributes them for every Android app that signs in with Google.
    allowUrl("https://developer.android.com/studio/terms.html") {
        because("Google Play services / Google ID for native Google sign-in (owner decision 2026-11-22)")
    }
    // Owner decision (android/GOAL.md "AI"): Gemini Nano through ML Kit GenAI, on the phone only.
    allowDependency("org.reactivestreams", "reactive-streams", "1.0.3") {
        because("CC0-1.0 (public domain dedication, no conditions), pulled in by ML Kit")
    }
    allowUrl("https://developers.google.com/ml-kit/terms") {
        because("ML Kit GenAI Prompt API for on-device Gemini Nano (owner decision 2026-11-22)")
    }
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
        // CI sets the version code for each upload (android-play.yml); the name is the release.
        versionCode = (System.getenv("HITOP_VERSION_CODE") ?: "1").toInt()
        versionName = "1.0.0"
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        // Screenshots and other files from UI tests land in
        // app/build/outputs/connected_android_test_additional_output (CI artifact "screenshots").
        testInstrumentationRunnerArguments["useTestStorageService"] = "true"
        ndk { abiFilters += listOf("arm64-v8a", "x86_64") }
        // The HITOP Cloud server's Google OAuth client id (type Web): Credential Manager asks Google
        // for an ID token for it (native sign-in). Empty = Google through the browser instead.
        buildConfigField("String", "GOOGLE_SERVER_CLIENT_ID", "\"" + (providers.gradleProperty("hitop.googleServerClientId").orNull ?: System.getenv("HITOP_GOOGLE_SERVER_CLIENT_ID") ?: "") + "\"")
    }

    // Release signing with the upload key (Google Play App Signing holds the app signing key).
    // From the environment only: CI decodes the keystore from a secret (android-play.yml,
    // release-android.yml); without it a release build is unsigned.
    val keystore = System.getenv("HITOP_UPLOAD_KEYSTORE")?.takeIf { file(it).exists() }
    signingConfigs {
        if (keystore != null) create("upload") {
            storeFile = file(keystore)
            storePassword = System.getenv("HITOP_UPLOAD_KEYSTORE_PASSWORD")
            keyAlias = System.getenv("HITOP_UPLOAD_KEY_ALIAS") ?: "upload"
            keyPassword = System.getenv("HITOP_UPLOAD_KEY_PASSWORD") ?: System.getenv("HITOP_UPLOAD_KEYSTORE_PASSWORD")
        }
    }
    buildTypes {
        release {
            // No shrinking: the JNI bridge and the JSON models are reached by name.
            isMinifyEnabled = false
            if (keystore != null) signingConfig = signingConfigs.getByName("upload")
        }
    }
    bundle {
        // One download per phone: its ABI's library only (Play splits by ABI).
        abi { enableSplit = true }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    buildFeatures {
        compose = true
        buildConfig = true
    }
    // libhitop_ffi.so per ABI from scripts/build-ffi.sh android.
    sourceSets["main"].jniLibs.directories.add(rootProject.layout.projectDirectory.dir("build/ffi/jniLibs").asFile.path)
    packaging { jniLibs { useLegacyPackaging = false } }
    // The UI tests in src/sharedTest run on the emulators (androidTest) and on the JVM under
    // Robolectric (test), with a host build of hitop-ffi: fast local runs, same tests.
    sourceSets["test"].kotlin.directories.add("src/sharedTest/kotlin")
    sourceSets["androidTest"].kotlin.directories.add("src/sharedTest/kotlin")
    // The published hitop sync test vectors, run on the device (VectorTest).
    sourceSets["androidTest"].assets.directories.add(rootProject.layout.projectDirectory.dir("../cli/crates/hitop-sync-crypto/vectors").asFile.path)
    testOptions {
        animationsDisabled = true
        unitTests {
            isIncludeAndroidResources = true
            all { test ->
                test.systemProperty("java.library.path", rootProject.layout.projectDirectory.dir("build/ffi/host").asFile.absolutePath)
                test.systemProperty("robolectric.graphicsMode", "NATIVE")
                // StoreScreenshots writes the Play listing's pictures there (android/store/screenshots).
                test.systemProperty("hitop.storeScreenshots", (project.findProperty("hitop.storeScreenshots") as String?) ?: "")
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
    implementation(libs.androidx.browser)
    implementation(libs.androidx.credentials)
    implementation(libs.androidx.credentials.play)
    implementation(libs.googleid)
    implementation(libs.glance.appwidget)
    implementation(libs.work.runtime)
    implementation(libs.glance.material3)
    implementation(libs.mlkit.genai.prompt)
    debugImplementation(libs.compose.ui.tooling)
    debugImplementation(libs.compose.ui.test.manifest)

    testImplementation(libs.junit)
    testImplementation(libs.robolectric)
    testImplementation(libs.glance.testing)
    testImplementation(libs.work.testing)
    testImplementation(platform(libs.compose.bom))
    testImplementation(libs.compose.ui.test.junit4)
    testImplementation(libs.androidx.test.ext.junit)
    testImplementation(libs.androidx.test.core)
    testImplementation(libs.androidx.test.storage)
    androidTestImplementation(platform(libs.compose.bom))
    androidTestImplementation(libs.compose.ui.test.junit4)
    androidTestImplementation(libs.compose.ui.test.accessibility)
    androidTestImplementation(libs.androidx.test.ext.junit)
    androidTestImplementation(libs.androidx.test.runner)
    androidTestImplementation(libs.androidx.test.core)
    androidTestImplementation(libs.androidx.test.storage)
    androidTestImplementation(libs.androidx.test.uiautomator)
    androidTestImplementation(testFixtures(project(":core")))
    androidTestUtil(libs.androidx.test.services)
}
