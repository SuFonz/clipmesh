package app.cm.clipmesh.bridge.foregroundservice

import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import android.util.Log
import androidx.core.content.ContextCompat
import app.cm.clipmesh.bridge.notification.ClipMeshNotifications

/**
 * Keeps the ClipMesh process alive while the app is in the background.
 *
 * Without a foreground service Android will freeze or kill the process within
 * minutes of the activity going away, which would silently stop clipboard sync
 * - the exact failure a user would never be able to diagnose.
 *
 * The service deliberately does **not** read the clipboard. Since Android 10 a
 * background app cannot, so the notification's action brings the activity
 * forward and the read happens there. See [ClipMeshNotifications].
 */
class ClipMeshService : Service() {

    companion object {
        private const val TAG = "ClipMeshService"

        /** Whether the service is alive, for the Rust side to query. */
        @Volatile
        var isRunning: Boolean = false
            private set

        /** Start the service, tolerating a missing notification permission. */
        fun start(context: Context) {
            try {
                ContextCompat.startForegroundService(
                    context,
                    Intent(context, ClipMeshService::class.java),
                )
            } catch (error: Exception) {
                // Android 12+ throws when the app is in the background and the
                // start is not allowed; the UI will offer to retry.
                Log.w(TAG, "could not start the foreground service", error)
            }
        }

        /** Stop the service. */
        fun stop(context: Context) {
            context.stopService(Intent(context, ClipMeshService::class.java))
        }
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        ClipMeshNotifications.ensureChannels(this)

        val notification = ClipMeshNotifications.serviceNotification(this, "Connected")

        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            startForeground(
                ClipMeshNotifications.SERVICE_NOTIFICATION_ID,
                notification,
                ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC,
            )
        } else {
            startForeground(ClipMeshNotifications.SERVICE_NOTIFICATION_ID, notification)
        }

        isRunning = true
        Log.i(TAG, "foreground service started")

        // START_STICKY so Android recreates us after reclaiming memory. The
        // engine reconnects on its own; nothing else has to be restored.
        return START_STICKY
    }

    override fun onDestroy() {
        isRunning = false
        Log.i(TAG, "foreground service stopped")
        super.onDestroy()
    }

    override fun onBind(intent: Intent?): IBinder? = null
}
