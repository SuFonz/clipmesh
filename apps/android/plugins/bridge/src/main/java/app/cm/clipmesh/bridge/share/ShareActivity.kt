package app.cm.clipmesh.bridge.share

import android.app.Activity
import android.content.Intent
import android.os.Bundle
import android.util.Log
import java.io.File

/**
 * Opens the share sheet for an image the notification is showing.
 *
 * ## Why this is an activity rather than a broadcast
 *
 * Android 10 and later refuse to let a background app start an activity, and a
 * notification action normally runs with the app in the background - that is the
 * whole point of a notification. An app *is* allowed to start an activity when
 * the start comes from a `PendingIntent` the system itself delivered for a
 * notification, and that is exactly what this is: the notification's share button
 * points here, and the chooser is started from here.
 *
 * It shows nothing of its own - the theme is translucent and it finishes as soon
 * as the chooser is on its way - so the user sees the picker and nothing else.
 * The same shape as `BroadcastActivity`, for the same reason: one activity, one
 * job, no UI.
 *
 * The path travels in the intent rather than being looked up from stored state,
 * so a notification whose notification has since been replaced can never share
 * the wrong image.
 */
class ShareActivity : Activity() {

    companion object {
        private const val TAG = "ClipMeshShare"

        /** Absolute path of the staged file to share. */
        const val EXTRA_SHARE_PATH = "app.cm.clipmesh.bridge.SHARE_PATH"
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        val path = intent?.getStringExtra(EXTRA_SHARE_PATH)
        if (path == null) {
            Log.w(TAG, "the share action arrived without a file")
            finish()
            return
        }

        val file = File(path)
        if (!file.isFile) {
            Log.w(TAG, "the staged image is gone: $path")
            finish()
            return
        }

        try {
            startActivity(SharedImages.chooserFor(this, file))
        } catch (error: Exception) {
            // Nothing left to try: the user tapped share and gets no sheet. Logged
            // rather than crashed - this runs in a user's app.
            Log.w(TAG, "could not open the share sheet", error)
        }

        finish()
    }
}
