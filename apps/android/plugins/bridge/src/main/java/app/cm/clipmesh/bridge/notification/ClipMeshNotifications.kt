package app.cm.clipmesh.bridge.notification

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import app.cm.clipmesh.bridge.broadcast.BroadcastActivity
import app.cm.clipmesh.bridge.mainActivityClass

/**
 * Every notification ClipMesh shows.
 *
 * There are exactly two, and they have different jobs:
 *
 *  * the **service** notification, which Android requires while a foreground
 *    service runs. It carries the "broadcast clipboard" action, which is the
 *    only way to trigger a broadcast from outside the app.
 *  * the **received** notification, shown when a peer sends something, with a
 *    preview and a copy action.
 */
object ClipMeshNotifications {

    /** Channel for the persistent foreground service notification. */
    const val CHANNEL_SERVICE = "clipmesh.service"

    /** Channel for "you received something" notifications. */
    const val CHANNEL_RECEIVED = "clipmesh.received"

    /** Intent action used by the broadcast button. */
    const val ACTION_BROADCAST = "app.cm.clipmesh.bridge.BROADCAST_CLIPBOARD"

    /** Intent action used by the copy button. */
    const val ACTION_COPY = "app.cm.clipmesh.bridge.COPY_CLIPBOARD"

    /** Intent extra carrying the id of the entry to copy. */
    const val EXTRA_ENTRY_ID = "entryId"

    /** Notification id of the foreground service notification. */
    const val SERVICE_NOTIFICATION_ID = 1001

    /** Notification id of the "received" notification. */
    const val RECEIVED_NOTIFICATION_ID = 1002

    /** Create both channels. Safe to call repeatedly. */
    fun ensureChannels(context: Context) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return

        val manager = context.getSystemService(NotificationManager::class.java) ?: return

        manager.createNotificationChannel(
            NotificationChannel(
                CHANNEL_SERVICE,
                "Background sync",
                NotificationManager.IMPORTANCE_MIN,
            ).apply {
                description = "Keeps ClipMesh connected to your other devices."
                setShowBadge(false)
            },
        )

        manager.createNotificationChannel(
            NotificationChannel(
                CHANNEL_RECEIVED,
                "Received clipboard",
                NotificationManager.IMPORTANCE_DEFAULT,
            ).apply {
                description = "Shown when another device sends you something."
            },
        )
    }

    /**
     * The persistent notification that keeps the process alive.
     *
     * `IMPORTANCE_MIN` on purpose: this is infrastructure, not news.
     */
    fun serviceNotification(context: Context, status: String): Notification {
        val launch = PendingIntent.getActivity(
            context,
            0,
            Intent(context, mainActivityClass(context)).apply {
                flags = Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP
            },
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )

        // NOTE: this opens a *transparent* activity, which reads the clipboard
        // while it holds focus and shows the user nothing. Android 10+ forbids
        // background clipboard reads outright, so a notification button that
        // worked without any activity at all is not implementable without an
        // accessibility service - a far bigger privacy ask than the feature is
        // worth. If that activity cannot do its job it falls back to bringing
        // the application forward, which is what this used to do directly.
        val broadcast = PendingIntent.getActivity(
            context,
            1,
            Intent(context, BroadcastActivity::class.java).apply {
                action = ACTION_BROADCAST
                flags = Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_NO_ANIMATION
            },
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )

        return NotificationCompat.Builder(context, CHANNEL_SERVICE)
            .setContentTitle("ClipMesh is running")
            .setContentText(status)
            .setSmallIcon(android.R.drawable.stat_sys_upload)
            .setContentIntent(launch)
            .addAction(android.R.drawable.ic_menu_share, "Broadcast clipboard", broadcast)
            .setOngoing(true)
            .setSilent(true)
            .setPriority(NotificationCompat.PRIORITY_MIN)
            .build()
    }

    /** Shown when a peer's clipboard arrives. */
    fun receivedNotification(
        context: Context,
        title: String,
        preview: String,
        isImage: Boolean,
    ): Notification {
        val launch = PendingIntent.getActivity(
            context,
            2,
            Intent(context, mainActivityClass(context)).apply {
                flags = Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP
            },
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )

        return NotificationCompat.Builder(context, CHANNEL_RECEIVED)
            .setContentTitle(title)
            .setContentText(preview)
            .setStyle(
                NotificationCompat.BigTextStyle().bigText(preview),
            )
            .setSmallIcon(android.R.drawable.ic_menu_edit)
            .setContentIntent(launch)
            .setAutoCancel(true)
            .build()
    }

    /** Post a notification, swallowing the "permission missing" case. */
    fun post(context: Context, id: Int, notification: Notification) {
        try {
            NotificationManagerCompat.from(context).notify(id, notification)
        } catch (error: SecurityException) {
            // Android 13+ without POST_NOTIFICATIONS. The service still runs;
            // the user simply sees nothing.
            android.util.Log.w("ClipMeshNotify", "notification permission is missing", error)
        }
    }
}
