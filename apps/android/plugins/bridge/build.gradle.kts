// ClipMesh Android native plugin.
//
// A Gradle *library* module rather than sources dropped into the generated app
// project, so the plugin keeps its own tests, manifest and permissions instead
// of being tangled into `gen/android` (which `tauri android init` may rewrite).
//
// It is wired into the generated project by two lines:
//   gen/android/settings.gradle      -> include ':bridge'
//   gen/android/app/build.gradle.kts -> implementation(project(":bridge"))
// See docs/BUILD.md.

plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
}

android {
    // Must match the application module; see gen/android/app/build.gradle.kts.
    compileSdk = 37
    namespace = "app.cm.clipmesh.bridge"

    defaultConfig {
        minSdk = 24

        // What a *consumer's* R8 must not strip out of this module: the argument
        // classes `Invoke.parseArgs` deserialises by reflection, which the app's
        // release build otherwise renames and guts - see consumer-rules.pro and
        // docs/BUILD.md. Declared here rather than in the app module's
        // `gen/android/app/proguard-rules.pro` because the rules describe this
        // module's classes, they travel with it, and `tauri android init` may
        // rewrite that generated file at any time.
        consumerProguardFiles("consumer-rules.pro")
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_1_8
        targetCompatibility = JavaVersion.VERSION_1_8
    }
}

kotlin {
    compilerOptions {
        jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_1_8)
    }
}

dependencies {
    // `app.tauri.plugin.*` and the `@TauriPlugin` annotation come from here.
    implementation(project(":tauri-android"))
    implementation("androidx.core:core-ktx:1.13.1")
}
