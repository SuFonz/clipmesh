package app.cm.clipmesh.bridge.foregroundservice

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.util.Log

/**
 * Restarts the foreground service after a reboot.
 *
 * Without this, a phone that restarts stops syncing until the user opens the
 * app - which they will not do, because there is nothing to see. Only started
 * when the user has left the foreground service enabled.
 */
class BootReceiver : BroadcastReceiver() {

    companion object {
        private const val TAG = "ClipMeshBoot"

        /** Preference file shared with the plugin. */
        const val PREFS = "clipmesh"

        /** Whether the user wants the service running. */
        const val PREF_KEEP_RUNNING = "foregroundServiceEnabled"
    }

    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action != Intent.ACTION_BOOT_COMPLETED) return

        val wanted = context
            .getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .getBoolean(PREF_KEEP_RUNNING, true)

        if (!wanted) {
            Log.i(TAG, "the user disabled background sync; not restarting")
            return
        }

        Log.i(TAG, "restarting the foreground service after boot")
        ClipMeshService.start(context)
    }
}
