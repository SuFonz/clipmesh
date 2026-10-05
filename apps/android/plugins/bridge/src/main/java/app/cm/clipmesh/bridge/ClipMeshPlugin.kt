package app.cm.clipmesh.bridge

import android.Manifest
import android.app.Activity
import android.content.Intent
import android.content.pm.PackageManager
import android.graphics.Bitmap
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.util.Log
import androidx.core.content.ContextCompat
import androidx.core.content.FileProvider
import app.cm.clipmesh.bridge.clipboard.ClipboardAccess
import app.cm.clipmesh.bridge.foregroundservice.BootReceiver
import app.cm.clipmesh.bridge.foregroundservice.ClipMeshService
import app.cm.clipmesh.bridge.notification.ClipMeshNotifications
import app.tauri.annotation.Command
import app.tauri.annotation.Permission
import app.tauri.annotation.PermissionCallback
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.io.ByteArrayOutputStream
import java.io.File
import java.io.FileOutputStream

/**
 * Alias for the notification permission.
 *
 * Declared at file level because an annotation argument has to be a compile
 * time constant, and it is referenced from the `@TauriPlugin` annotation below.
 */
private const val NOTIFICATION_PERMISSION_ALIAS = "notifications"

/**
 * The Kotlin half of the ClipMesh Android plugin.
 *
 * The Rust half talks to this class through `PluginHandle::run_mobile_plugin`,
 * so every `@Command` here has a matching typed method in
 * `apps/android/src-tauri/src/plugin.rs`. Changing a name on one side without
 * the other is a runtime failure, not a compile error - the names are therefore
 * listed in `docs/IPC.md`.
 *
 * All clipboard work happens on the main looper, because
 * `ClipboardManager` requires it.
 */
@TauriPlugin(
    permissions = [
        Permission(
            alias = NOTIFICATION_PERMISSION_ALIAS,
            strings = [Manifest.permission.POST_NOTIFICATIONS],
        ),
    ],
)
class ClipMeshPlugin(private val activity: Activity) : Plugin(activity) {

    companion object {
        private const val TAG = "ClipMeshPlugin"
        private const val CLIPBOARD_DIR = "clipboard"
        private const val CLIPBOARD_FILE = "clipmesh-clipboard.png"
    }

    private val main = Handler(Looper.getMainLooper())

    // -----------------------------------------------------------------------
    // Clipboard
    // -----------------------------------------------------------------------

    /**
     * Read the clipboard and hand it to Rust.
     *
     * Returns `{kind: "empty"}` when the clipboard holds nothing ClipMesh
     * carries *or* when Android refuses the read because the app has no focus -
     * the two are indistinguishable here, and a background app cannot broadcast
     * anyway.
     */
    @Command
    fun readClipboard(invoke: Invoke) {
        onMain {
            when (val content = ClipboardAccess.read(activity)) {
                is ClipboardAccess.Content.Text -> invoke.resolve(
                    JSObject().apply {
                        put("kind", "text")
                        put("text", content.text)
                    },
                )

                is ClipboardAccess.Content.Image -> {
                    val encoded = content.bitmap.toPngBase64()
                    if (encoded == null) {
                        invoke.resolve(emptyPayload())
                    } else {
                        invoke.resolve(
                            JSObject().apply {
                                put("kind", "image")
                                put("png", encoded)
                                put("width", content.bitmap.width)
                                put("height", content.bitmap.height)
                            },
                        )
                    }
                }

                ClipboardAccess.Content.Empty -> invoke.resolve(emptyPayload())
            }
        }
    }

    /** Write text received from a peer onto the system clipboard. */
    @Command
    fun setText(invoke: Invoke) {
        val text = invoke.parseArgs(SetTextArgs::class.java).text.orEmpty()
        onMain {
            if (ClipboardAccess.writeText(activity, text)) {
                invoke.resolve()
            } else {
                invoke.reject("the clipboard could not be written")
            }
        }
    }

    /**
     * Write an image received from a peer onto the system clipboard.
     *
     * `setPrimaryClip` cannot carry a bitmap, so the standard route is used: the
     * PNG is written into the cache directory, exposed through the FileProvider
     * the generated manifest already declares, and put on the clipboard as a
     * content URI.
     */
    @Command
    fun setImage(invoke: Invoke) {
        val encoded = invoke.parseArgs(SetImageArgs::class.java).png
        if (encoded == null) {
            invoke.reject("no image was supplied")
            return
        }

        val bitmap = ClipboardAccess.decodeBase64Png(encoded)
        if (bitmap == null) {
            invoke.reject("the image could not be decoded")
            return
        }

        onMain {
            val uri = writeToCache(bitmap)
            if (uri == null) {
                invoke.reject("the image could not be written to the cache")
                return@onMain
            }

            val clipboard = activity.getSystemService(android.content.ClipboardManager::class.java)
            if (clipboard == null) {
                invoke.reject("the clipboard service is unavailable")
                return@onMain
            }

            try {
                clipboard.setPrimaryClip(
                    android.content.ClipData.newUri(activity.contentResolver, "ClipMesh", uri),
                )
                invoke.resolve()
            } catch (error: Exception) {
                Log.w(TAG, "could not put the image on the clipboard", error)
                invoke.reject("the clipboard rejected the image")
            }
        }
    }

    // -----------------------------------------------------------------------
    // Notifications
    // -----------------------------------------------------------------------

    /** Show the "you received something" notification. */
    @Command
    fun showReceived(invoke: Invoke) {
        val args = invoke.parseArgs(ShowReceivedArgs::class.java)
        ClipMeshNotifications.ensureChannels(activity)
        ClipMeshNotifications.post(
            activity,
            ClipMeshNotifications.RECEIVED_NOTIFICATION_ID,
            ClipMeshNotifications.receivedNotification(
                activity,
                args.title ?: "Clipboard received",
                args.preview.orEmpty(),
                args.isImage,
            ),
        )
        invoke.resolve()
    }

    /**
     * Ask for POST_NOTIFICATIONS.
     *
     * Below Android 13 there is no runtime permission to request, so the answer
     * is simply whether notifications are enabled.
     *
     * The request goes through Tauri's permission plumbing rather than
     * `ActivityCompat`: the alias is declared on the `@TauriPlugin` annotation
     * above, and the user's answer arrives in [notificationPermissionResult].
     * Requesting it by hand would need the host activity to forward
     * `onRequestPermissionsResult`, which a library module cannot hook into.
     */
    @Command
    fun requestNotificationPermission(invoke: Invoke) {
        if (hasNotificationPermission()) {
            invoke.resolve(granted(true))
            return
        }

        requestPermissionForAlias(
            NOTIFICATION_PERMISSION_ALIAS,
            invoke,
            "notificationPermissionResult",
        )
    }

    /** Runs once the user has answered the POST_NOTIFICATIONS dialog. */
    @PermissionCallback
    fun notificationPermissionResult(invoke: Invoke) {
        invoke.resolve(granted(hasNotificationPermission()))
    }

    private fun hasNotificationPermission(): Boolean =
        Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU ||
            ContextCompat.checkSelfPermission(
                activity,
                Manifest.permission.POST_NOTIFICATIONS,
            ) == PackageManager.PERMISSION_GRANTED

    // -----------------------------------------------------------------------
    // Foreground service
    // -----------------------------------------------------------------------

    /** Start the service that keeps the process alive. */
    @Command
    fun startService(invoke: Invoke) {
        activity.getSharedPreferences(BootReceiver.PREFS, Activity.MODE_PRIVATE)
            .edit()
            .putBoolean(BootReceiver.PREF_KEEP_RUNNING, true)
            .apply()

        ClipMeshService.start(activity)
        invoke.resolve()
    }

    /** Stop the foreground service. */
    @Command
    fun stopService(invoke: Invoke) {
        activity.getSharedPreferences(BootReceiver.PREFS, Activity.MODE_PRIVATE)
            .edit()
            .putBoolean(BootReceiver.PREF_KEEP_RUNNING, false)
            .apply()

        ClipMeshService.stop(activity)
        invoke.resolve()
    }

    /** Whether the foreground service is alive. */
    @Command
    fun serviceRunning(invoke: Invoke) {
        invoke.resolve(
            JSObject().apply {
                put("running", ClipMeshService.isRunning)
            },
        )
    }

    // -----------------------------------------------------------------------
    // Notification actions
    // -----------------------------------------------------------------------

    /**
     * Called by the Tauri activity when it is launched or re-launched from a
     * notification action.
     *
     * Android 10+ will not let a background app read the clipboard, so the
     * "broadcast clipboard" button brings the app forward and the work happens
     * here, where the app has focus again.
     */
    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        when (intent.action) {
            ClipMeshNotifications.ACTION_BROADCAST -> {
                Log.i(TAG, "forwarding the broadcast request to the UI")
                trigger("broadcast-clipboard", JSObject())
            }
            ClipMeshNotifications.ACTION_COPY -> {
                val id = intent.getStringExtra(ClipMeshNotifications.EXTRA_ENTRY_ID)
                trigger(
                    "copy-entry",
                    JSObject().apply { put("entryId", id) },
                )
            }
            else -> Unit
        }
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    private fun onMain(block: () -> Unit) {
        if (Looper.myLooper() == Looper.getMainLooper()) block() else main.post(block)
    }

    private fun emptyPayload(): JSObject = JSObject().apply { put("kind", "empty") }

    private fun granted(value: Boolean): JSObject =
        JSObject().apply { put("granted", value) }

    private fun Bitmap.toPngBase64(): String? = try {
        val stream = ByteArrayOutputStream()
        compress(Bitmap.CompressFormat.PNG, 100, stream)
        android.util.Base64.encodeToString(stream.toByteArray(), android.util.Base64.NO_WRAP)
    } catch (error: Exception) {
        Log.w(TAG, "could not encode a bitmap as PNG", error)
        null
    }

    /** Write the bitmap into the FileProvider-backed cache directory. */
    private fun writeToCache(bitmap: Bitmap): android.net.Uri? = try {
        val directory = File(activity.cacheDir, CLIPBOARD_DIR)
        if (!directory.exists() && !directory.mkdirs()) {
            Log.w(TAG, "could not create ${directory.absolutePath}")
        }
        val file = File(directory, CLIPBOARD_FILE)
        FileOutputStream(file).use { output ->
            bitmap.compress(Bitmap.CompressFormat.PNG, 100, output)
        }
        FileProvider.getUriForFile(activity, "${activity.packageName}.fileprovider", file)
    } catch (error: Exception) {
        Log.w(TAG, "could not stage the image for the clipboard", error)
        null
    }
}

// ---------------------------------------------------------------------------
// Command arguments
// ---------------------------------------------------------------------------
//
// Tauri deserialises these with Jackson (`Invoke.parseArgs`), so they are plain
// classes with mutable nullable properties. They deliberately do NOT extend
// `JSObject`: that class is final, and it is the JSON *writer* used by
// `resolve`, not something to model input with.

/** Arguments for [ClipMeshPlugin.setText]. */
class SetTextArgs {
    var text: String? = null
}

/** Arguments for [ClipMeshPlugin.setImage]. */
class SetImageArgs {
    var png: String? = null
}

/** Arguments for [ClipMeshPlugin.showReceived]. */
class ShowReceivedArgs {
    var title: String? = null

    var preview: String? = null

    var isImage: Boolean = false
}
