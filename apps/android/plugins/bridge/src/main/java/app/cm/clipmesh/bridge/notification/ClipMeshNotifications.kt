package app.cm.clipmesh.bridge.notification

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.graphics.Bitmap
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import app.cm.clipmesh.bridge.R
import app.cm.clipmesh.bridge.broadcast.BroadcastActivity
import app.cm.clipmesh.bridge.mainActivityClass
import app.cm.clipmesh.bridge.share.ShareActivity
import java.io.File

/**
 * Every notification ClipMesh shows.
 *
 * There are exactly two, and they have different jobs:
 *
 *  * the **service** notification, which Android requires while a foreground
 *    service runs. It carries the "broadcast clipboard" action, which is the
 *    only way to trigger a broadcast from outside the app.
 *  * the **received** notification, shown when a peer sends something. Text is
 *    previewed as text; an image is previewed as the image itself and gains a
 *    share action, because for a picture "copy" is rarely what the user wants to
 *    do with it. A text that arrives later replaces the notification, and with it
 *    the image preview and its share button - there is no state to clean up,
 *    because the notification *is* the state.
 *
 * One notification id covers both shapes of the received notification on purpose:
 * the newest thing that arrived is the only thing worth showing, and Android
 * replacing the old notification is what makes the image preview disappear when
 * text arrives.
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
            // Our own monochrome glyph. The template used
            // `android.R.drawable.stat_sys_upload`, the system's up-arrow that
            // means "a transfer is in progress" - so a notification that is
            // permanently on screen looked like an upload that never finished.
            .setSmallIcon(R.drawable.ic_clipmesh_status)
            .setContentIntent(launch)
            .addAction(R.drawable.ic_clipmesh_status, "Broadcast clipboard", broadcast)
            .setOngoing(true)
            .setSilent(true)
            .setPriority(NotificationCompat.PRIORITY_MIN)
            .build()
    }

    /**
     * Shown when text arrives (or when an image has no stored copy to preview).
     *
     * There is no "this is an image" flag: text and image have their own call
     * sites, and the only thing that differed was the preview line, which Rust
     * already words for each case.
     */
    fun receivedNotification(
        context: Context,
        title: String,
        preview: String,
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
            .setSmallIcon(R.drawable.ic_clipmesh_status)
            .setContentIntent(launch)
            .setAutoCancel(true)
            .build()
    }

    /**
     * Shown when a peer sends an image.
     *
     * The bitmap is the notification's own preview - it is already decoded, so
     * drawing it costs nothing - and [staged] is where the share action reads the
     * image from. Nothing else is remembered: when a text replaces this
     * notification, the preview and the action are gone with it, which is the
     * behaviour the user asked for expressed as the absence of state rather than
     * as cleanup code.
     *
     * The action label is English, like every other string this module puts in the
     * shade (`ClipMesh is running`, `Broadcast clipboard`, the channel names): the
     * notification is built by Kotlin while the app may not be running at all, so
     * it cannot come from the webview's i18n catalog. The title and the preview
     * line *are* localisable and come from Rust.
     */
    fun receivedImageNotification(
        context: Context,
        title: String,
        preview: String,
        previewBitmap: Bitmap,
        staged: File,
    ): Notification {
        val launch = PendingIntent.getActivity(
            context,
            2,
            Intent(context, mainActivityClass(context)).apply {
                flags = Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP
            },
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )

        // Starts ShareActivity, which opens the chooser. A notification action
        // cannot open a chooser by itself: only an activity started from a
        // notification's PendingIntent is exempt from the background
        // activity-launch restrictions, and the chooser has to be started by an
        // app that is allowed to.
        val share = PendingIntent.getActivity(
            context,
            3,
            Intent(context, ShareActivity::class.java).apply {
                action = Intent.ACTION_SEND
                putExtra(ShareActivity.EXTRA_SHARE_PATH, staged.absolutePath)
                flags = Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_NO_ANIMATION
            },
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )

        return NotificationCompat.Builder(context, CHANNEL_RECEIVED)
            .setContentTitle(title)
            .setContentText(preview)
            .setLargeIcon(previewBitmap)
            .setStyle(
                NotificationCompat.BigPictureStyle()
                    .bigPicture(previewBitmap)
                    .bigLargeIcon(null as Bitmap?),
            )
            .setSmallIcon(R.drawable.ic_clipmesh_status)
            .setContentIntent(launch)
            .addAction(R.drawable.ic_clipmesh_status, "Share", share)
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
