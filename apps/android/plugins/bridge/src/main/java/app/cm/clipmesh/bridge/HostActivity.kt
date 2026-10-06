package app.cm.clipmesh.bridge

import android.content.Context

/**
 * The application's own activity, resolved by name.
 *
 * Resolved reflectively because this module is a library: it must not
 * compile-depend on the generated application module, which would be a circular
 * Gradle dependency. The name is the one Tauri generates
 * (`gen/android/app/src/main/java/<package>/MainActivity.kt`).
 */
internal fun mainActivityClass(context: Context): Class<*> =
    Class.forName("${context.packageName}.MainActivity")
