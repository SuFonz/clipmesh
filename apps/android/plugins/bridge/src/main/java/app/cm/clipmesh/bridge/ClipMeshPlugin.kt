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
import app.cm.clipmesh.bridge.broadcast.BroadcastHandoff
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
 * listed in `docs/BUILD.md`.
 *
 * Everything that touches `ClipboardManager` runs on the main looper, because
 * the platform requires it. Reading the process-wide broadcast handoff does not
 * need one, and does not take one: it is polled while the app is in the
 * background, where the main looper has better things to do.
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

    init {
        // Rust registers this class during its own setup, so this is the moment
        // the process can be said to have a host that is able to collect a
        // broadcast handoff. `BroadcastActivity` checks the same flag: a process
        // with no host would leave a request to rot.
        BroadcastHandoff.attachHost()

        // A process started *by* the notification action has no `onNewIntent` -
        // the broadcast intent is the launch intent. Without this the visible
        // fallback would bring the app up and then do nothing, which is exactly
        // the failure it exists to avoid.
        if (activity.intent?.action == ClipMeshNotifications.ACTION_BROADCAST) {
            Log.i(TAG, "the app was started by the broadcast action")
            BroadcastHandoff.deposit(BroadcastHandoff.Request.Visible)
        }
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
            invoke.resolve(contentToJson(ClipboardAccess.read(activity)))
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

    /**
     * Send the app to the back of the task stack.
     *
     * The ending of the **visible fallback**: the notification's action normally
     * reads the clipboard from a transparent activity that shows nothing, but
     * when that cannot do its job it brings this activity forward instead, and
     * the user has to end up back in whatever they were doing rather than being
     * left inside ClipMesh.
     *
     * `moveTaskToBack` rather than `finish()`: the process, and with it the
     * engine, has to stay alive behind the foreground service.
     */
    @Command
    fun leaveApp(invoke: Invoke) {
        activity.moveTaskToBack(true)
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
     * Called by the Tauri activity when a notification action relaunches it.
     *
     * This is the **visible fallback**: `BroadcastActivity` either never got
     * focus or found nothing readable on the clipboard, so it brought the real
     * activity forward instead of quietly giving up. Nothing is read here -
     * Android will not hand the clipboard to an app that is still starting - the
     * request is left in the handoff marked visible, and the Rust host reads
     * through this activity once it is up.
     *
     * This used to call `trigger("broadcast-clipboard")`. That went nowhere:
     * `trigger` only delivers to `Channel`s registered through the plugin's
     * `registerListener`, and our plugin is not in the Tauri ACL, so the UI has
     * no way to register one. The button brought the app up and did nothing else.
     */
    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        if (intent.action != ClipMeshNotifications.ACTION_BROADCAST) return

        Log.i(TAG, "the notification asked for a visible broadcast")
        BroadcastHandoff.deposit(BroadcastHandoff.Request.Visible)
    }

    /**
     * Collect a broadcast the notification asked for.
     *
     * Answers `{"requested": false}` when there is nothing to do, which is the
     * usual answer: the Rust host polls this. `visible` says whether the user is
     * looking at the app because of the request, `payload` carries what
     * `BroadcastActivity` read when the request never had to become visible, and
     * `serviceRunning` tells the poller whether the notification - the only
     * thing that can start a request - is still up.
     *
     * A request is answered at most once: [BroadcastHandoff.take] clears it, so
     * a poll that runs twice cannot send twice.
     *
     * Resolved on the calling thread rather than the main looper. This only
     * reads a process-wide field, and a hop per poll would put every pickup
     * behind whatever the UI happens to be doing.
     */
    @Command
    fun takeBroadcast(invoke: Invoke) {
        val request = BroadcastHandoff.take()
        val response = JSObject().apply {
            put("requested", request != null)
            put("serviceRunning", ClipMeshService.isRunning)
        }

        when (request) {
            null -> Unit

            is BroadcastHandoff.Request.Read -> {
                response.put("visible", false)
                response.put("payload", contentToJson(request.content))
            }

            BroadcastHandoff.Request.Visible -> response.put("visible", true)
        }

        invoke.resolve(response)
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    /**
     * The wire shape of a clipboard read, shared by [readClipboard] and
     * [takeBroadcast] so both ends see the same `{"kind": …}` object that
     * `ClipboardPayload` in `plugin.rs` expects.
     *
     * Base64 is only ever used across the JNI boundary; the network protocol
     * carries raw PNG bytes.
     */
    private fun contentToJson(content: ClipboardAccess.Content): JSObject = when (content) {
        is ClipboardAccess.Content.Text -> JSObject().apply {
            put("kind", "text")
            put("text", content.text)
        }

        is ClipboardAccess.Content.Image -> {
            val encoded = content.bitmap.toPngBase64()
            if (encoded == null) {
                emptyPayload()
            } else {
                JSObject().apply {
                    put("kind", "image")
                    put("png", encoded)
                    put("width", content.bitmap.width)
                    put("height", content.bitmap.height)
                }
            }
        }

        ClipboardAccess.Content.Empty -> emptyPayload()
    }

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
