package app.cm.clipmesh.bridge.foregroundservice

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.util.Log
import app.cm.clipmesh.bridge.notification.ClipMeshNotifications

/**
 * Receives the notification's actions.
 *
 * Tapping **Broadcast clipboard** cannot read the clipboard here: Android 10
 * and later return null for any background read, and this receiver runs in the
 * background by definition. So the receiver's job is to bring the activity
 * forward with [ClipMeshNotifications.ACTION_BROADCAST] set, and the plugin
 * picks it up in `onNewIntent` once the app has focus.
 *
 * The receiver exists rather than putting the `PendingIntent` straight on the
 * activity because a broadcast survives the activity being recreated, and it
 * gives one place to log what the user asked for.
 */
class ClipboardActionReceiver : BroadcastReceiver() {

    companion object {
        private const val TAG = "ClipMeshAction"
    }

    override fun onReceive(context: Context, intent: Intent) {
        when (intent.action) {
            ClipMeshNotifications.ACTION_BROADCAST -> {
                Log.i(TAG, "the user asked to broadcast the clipboard")
                forwardToActivity(context, ClipMeshNotifications.ACTION_BROADCAST)
            }

            ClipMeshNotifications.ACTION_COPY -> {
                val entryId = intent.getStringExtra(ClipMeshNotifications.EXTRA_ENTRY_ID)
                Log.i(TAG, "the user asked to copy entry $entryId")
                forwardToActivity(context, ClipMeshNotifications.ACTION_COPY, entryId)
            }

            else -> Log.d(TAG, "ignoring action ${intent.action}")
        }
    }

    private fun forwardToActivity(context: Context, action: String, entryId: String? = null) {
        val launch = Intent(context, ClipMeshNotifications::class.java).let {
            Intent(context, mainActivityClass(context)).apply {
                this.action = action
                flags = Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP
                if (entryId != null) putExtra(ClipMeshNotifications.EXTRA_ENTRY_ID, entryId)
            }
        }
        context.startActivity(launch)
    }

    private fun mainActivityClass(context: Context): Class<*> =
        Class.forName("${context.packageName}.MainActivity")
}
