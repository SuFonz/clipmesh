package app.cm.clipmesh.bridge.broadcast

import android.app.Activity
import android.content.Intent
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.util.Log
import app.cm.clipmesh.bridge.clipboard.ClipboardAccess
import app.cm.clipmesh.bridge.mainActivityClass
import app.cm.clipmesh.bridge.notification.ClipMeshNotifications

/**
 * Reads the clipboard for the notification action without showing anything.
 *
 * Android 10 and later refuse to hand the clipboard to an app that is not in
 * the foreground, so the action has to start *something*. An activity with a
 * translucent theme is the smallest thing that can hold window focus: the
 * system counts it as foreground, which is what the read needs, while the user
 * sees nothing but their own screen - which is what they asked for.
 *
 * The read happens in [onWindowFocusChanged], never in `onCreate`: a window
 * that has not been given focus yet gets nothing back from `ClipboardManager`,
 * which is the whole reason this activity exists.
 *
 * ## When it cannot do its job
 *
 * Three things can go wrong, and only the first two are observable from here:
 *
 *  * **It is never given focus.** The timeout in [onCreate] fires and the
 *    request goes down the visible path.
 *  * **It has focus but the read yields nothing usable.** Also the visible
 *    path: the real activity is brought forward, reads the clipboard again with
 *    a fully resumed window, and the user is told what happened if it is still
 *    empty.
 *  * **It is never started at all.** Android or an OEM ROM can refuse a
 *    background activity start, and a refused start is not reported to the app
 *    that asked for it. Nothing here can observe that, so nothing here can
 *    recover from it: the button does nothing, and the in-app "broadcast
 *    clipboard" button is the way out.
 *
 * The visible path is the behaviour the notification had before this activity
 * existed, which is what makes it a fallback rather than a new mode.
 */
class BroadcastActivity : Activity() {

    companion object {
        private const val TAG = "ClipMeshBroadcast"

        /**
         * How long to wait for window focus before falling back.
         *
         * A device that never grants focus must not leave an invisible activity
         * sitting in the task stack. Two seconds is far longer than a focus
         * change takes and far shorter than a user's patience.
         */
        private const val FOCUS_TIMEOUT_MS = 2_000L
    }

    private val timeout = Handler(Looper.getMainLooper())

    /** Set by whichever ending runs first, so the other one never does. */
    private var settled = false

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        // No content view: the window exists only to hold focus, and the
        // activity's theme is transparent, so there is nothing to draw.
        timeout.postDelayed({
            Log.w(TAG, "the window never gained focus; falling back to the visible path")
            fallBack()
        }, FOCUS_TIMEOUT_MS)
    }

    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        // Focus can be granted more than once; the first ending is the only one
        // that counts, and re-reading the clipboard after it would be pointless.
        if (!hasFocus || settled) return

        val content = ClipboardAccess.read(this)
        if (content == ClipboardAccess.Content.Empty) {
            Log.w(TAG, "the clipboard held nothing readable; falling back to the visible path")
            fallBack()
            return
        }

        if (!BroadcastHandoff.hostAttached) {
            // The foreground service, and with it the notification, outlived
            // the app: there is no Rust in this process to collect a handoff.
            // The visible path starts the host, which is what makes the tap
            // work at all.
            Log.w(TAG, "no host is attached to collect the broadcast; falling back")
            fallBack()
            return
        }

        settle {
            BroadcastHandoff.deposit(BroadcastHandoff.Request.Read(content))
        }
    }

    override fun onDestroy() {
        timeout.removeCallbacksAndMessages(null)
        super.onDestroy()
    }

    /**
     * Hand the request to the visible path.
     *
     * The real activity is brought forward with the action the plugin already
     * understands: the user sees ClipMesh for a moment, the Rust host reads the
     * clipboard through the focused activity, sends it, and puts the app back
     * where it was.
     */
    private fun fallBack() {
        val launch = Intent(this, mainActivityClass(this)).apply {
            action = ClipMeshNotifications.ACTION_BROADCAST
            flags = Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP
        }

        settle {
            try {
                startActivity(launch)
            } catch (error: Exception) {
                // Android 10 and later refuse a background activity start
                // without an exemption. There is nothing else left to try.
                Log.w(TAG, "could not bring the app forward", error)
            }
        }
    }

    /** Run one ending, finish, and make sure no other ending ever runs. */
    private fun settle(ending: () -> Unit) {
        if (settled) return
        settled = true
        ending()
        finish()
    }
}
