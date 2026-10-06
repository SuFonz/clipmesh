import java.util.Properties
import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("rust")
}

val tauriProperties = Properties().apply {
    val propFile = file("tauri.properties")
    if (propFile.exists()) {
        propFile.inputStream().use { load(it) }
    }
}

// AGP strips native libraries with the NDK's `llvm-strip`. With no NDK resolved
// it silently gives up:
//
//   Unable to strip the following libraries, packaging them as they are:
//   libclipmesh_android_lib.so
//
// and a *debug* Rust library then ships with all of its DWARF - 301 MB instead
// of ~26 MB. Release builds look fine either way because
// `[profile.release] strip = true` in the workspace Cargo.toml already stripped
// them in cargo, which is what made this easy to miss.
//
// Prefer the NDK the environment points at so a different machine works without
// editing this file; fall back to the version this project was built against.
//
// (`file(...)` rather than `java.io.File(...)`: inside a Gradle Kotlin DSL script
// the identifier `java` resolves to the Java project extension, not the package.)
val resolvedNdkVersion: String =
    System.getenv("ANDROID_NDK_HOME")?.let { file(it).name }
        ?: System.getenv("NDK_HOME")?.let { file(it).name }
        ?: "29.0.13846066"

// ---------------------------------------------------------------------------
// Release signing.
//
// The keystore and its passwords must never be committed, so all four values
// are read from *outside* the repository - a Gradle property first, then the
// environment. Putting them in the global `~/.gradle/gradle.properties` keeps
// them out of this project entirely and out of `git status`:
//
//     KEYSTORE_FILE       path to the .jks / .keystore (absolute, or relative
//                         to this module's directory)
//     KEYSTORE_PASSWORD   keystore password
//     KEY_ALIAS           alias of the key inside it
//     KEY_PASSWORD        that key's password
//
// When any of the four is missing the release build still *configures*, but the
// APK comes out unsigned and therefore cannot be installed - the warning below
// names the missing ones rather than letting you find out at install time.
// ---------------------------------------------------------------------------
fun signingSecret(name: String): String? =
    providers.gradleProperty(name).orNull?.takeIf { it.isNotBlank() }
        ?: System.getenv(name)?.takeIf { it.isNotBlank() }

// Deliberately not named `keyAlias` / `keyPassword`: inside `create("release")`
// below, the receiver has properties with those exact names, and a same-named
// local would turn each assignment into `x = x`.
val signingStoreFile = signingSecret("KEYSTORE_FILE")
val signingStorePassword = signingSecret("KEYSTORE_PASSWORD")
val signingKeyAlias = signingSecret("KEY_ALIAS")
val signingKeyPassword = signingSecret("KEY_PASSWORD")
val hasReleaseSigning =
    signingStoreFile != null && signingStorePassword != null &&
        signingKeyAlias != null && signingKeyPassword != null

if (!hasReleaseSigning) {
    val missing = listOf(
        "KEYSTORE_FILE" to signingStoreFile,
        "KEYSTORE_PASSWORD" to signingStorePassword,
        "KEY_ALIAS" to signingKeyAlias,
        "KEY_PASSWORD" to signingKeyPassword,
    ).filter { it.second == null }.joinToString(", ") { it.first }

    logger.lifecycle(
        "ClipMesh: release signing is not configured - missing $missing. " +
            "Release APKs/AABs will be UNSIGNED and cannot be installed. " +
            "Set them in ~/.gradle/gradle.properties or the environment.",
    )
}

android {
    compileSdk = 37
    namespace = "app.cm.clipmesh"
    ndkVersion = resolvedNdkVersion
    defaultConfig {
        manifestPlaceholders["usesCleartextTraffic"] = "false"
        applicationId = "app.cm.clipmesh"
        minSdk = 24
        targetSdk = 37
        versionCode = tauriProperties.getProperty("tauri.android.versionCode", "1").toInt()
        versionName = tauriProperties.getProperty("tauri.android.versionName", "1.0")
    }
    signingConfigs {
        if (hasReleaseSigning) {
            create("release") {
                storeFile = file(signingStoreFile!!)
                storePassword = signingStorePassword
                keyAlias = signingKeyAlias
                keyPassword = signingKeyPassword
            }
        }
    }
    buildTypes {
        getByName("debug") {
            manifestPlaceholders["usesCleartextTraffic"] = "true"
            isDebuggable = true
            isJniDebuggable = true
            isMinifyEnabled = false
            // NOTE: Tauri's template puts a `packaging { jniLibs.keepDebugSymbols }`
            // block here so native debugging works on device. It was removed on
            // purpose: it stopped AGP from stripping the Rust libraries, and an
            // unstripped debug libclipmesh_android_lib.so is ~300 MB, of which
            // ~275 MB is DWARF. The NDK's llvm-strip takes it to ~26 MB.
            //
            // Trade-off: no breakpoints inside Rust when running on a device. To
            // get them back for the ABI you are actually debugging, re-add just
            // that one line:
            //
            //     packaging { jniLibs.keepDebugSymbols.add("*/arm64-v8a/*.so") }
        }
        getByName("release") {
            // null when the four KEYSTORE_* / KEY_* values are absent, which
            // leaves the APK unsigned rather than failing the configuration.
            signingConfig = signingConfigs.findByName("release")
            optimization {
               enable = true
            }
            proguardFiles(
                *fileTree(".") {
                  include("**/*.pro")
                  exclude("build/**")
                }.files.toTypedArray()
            )
        }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_1_8
        targetCompatibility = JavaVersion.VERSION_1_8
    }
    buildFeatures {
        buildConfig = true
    }
}

kotlin {
    compilerOptions {
        jvmTarget = JvmTarget.JVM_1_8
    }
}

rust {
    rootDirRel = "../../../"
}

dependencies {
    // ClipMesh's clipboard, notification and foreground service plugin.
    // Declared in settings.gradle; see docs/MAINTENANCE.md §1.
    implementation(project(":bridge"))

    implementation("androidx.webkit:webkit:1.14.0")
    implementation("androidx.appcompat:appcompat:1.7.1")
    implementation("androidx.activity:activity-ktx:1.10.1")
    implementation("com.google.android.material:material:1.12.0")
    implementation("androidx.lifecycle:lifecycle-process:2.10.0")
    testImplementation("junit:junit:4.13.2")
    androidTestImplementation("androidx.test.ext:junit:1.1.4")
    androidTestImplementation("androidx.test.espresso:espresso-core:3.5.0")
}

apply(from = file("tauri.build.gradle.kts"))
