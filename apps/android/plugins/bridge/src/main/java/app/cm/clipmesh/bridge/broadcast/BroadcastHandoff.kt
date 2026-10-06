package app.cm.clipmesh.bridge.broadcast

import android.os.SystemClock
import android.util.Log
import app.cm.clipmesh.bridge.clipboard.ClipboardAccess

/**
 * Where a broadcast the user asked for waits for the Rust host.
 *
 * The notification's "broadcast clipboard" action opens [BroadcastActivity],
 * which reads the clipboard and finishes. From there the request cannot go
 * anywhere on its own: Rust can call Kotlin, but Kotlin cannot call Rust
 * (Tauri's Android API is one-way, and `Plugin.trigger` needs a JS listener
 * that this plugin is not registered for), so the result is parked here until
 * the host's poll collects it.
 *
 * **Process-wide on purpose.** The plugin instance belongs to the Tauri
 * activity, and a broadcast is usually requested while that activity is gone
 * and the process is kept alive by the foreground service alone - the handoff
 * has to outlive it.
 */
object BroadcastHandoff {

    private const val TAG = "ClipMeshBroadcast"

    /**
     * How long a request stays collectable.
     *
     * Comfortably longer than the host's slow poll, so a request cannot expire
     * between two ticks, and short enough that a request deposited in a process
     * with no host to collect it - the activity is reachable from a notification
     * that outlived the app - is dropped instead of being broadcast minutes
     * later, when the user next opens ClipMesh and the clipboard has moved on.
     */
    private const val FRESH_FOR_MS = 15_000L

    /** A broadcast the user asked for, and how it was started. */
    sealed class Request {
        /**
         * Whether the user is looking at ClipMesh because of this request.
         *
         * The fallback brings the real activity forward, the transparent one
         * shows nothing at all. Whoever performs the broadcast reads this to
         * decide whether the user has to be sent back afterwards - and it is
         * explicitly part of the request rather than inferred, because sending
         * the app back when the user was already using it would be wrong.
         */
        abstract val visible: Boolean

        /** [BroadcastActivity] read the clipboard while it held focus. */
        data class Read(val content: ClipboardAccess.Content) : Request() {
            override val visible = false
        }

        /** ClipMesh was brought to the front; the read happens through it. */
        object Visible : Request() {
            override val visible = true
        }
    }

    @Volatile
    private var pending: Request? = null

    @Volatile
    private var depositedAt = 0L

    /**
     * Whether Rust has registered the plugin in this process.
     *
     * A handoff deposited before that can never be collected: a process started
     * by [BroadcastActivity] alone never runs the Tauri setup. The activity
     * checks this and falls back to the visible path, which does start the host.
     */
    @Volatile
    var hostAttached: Boolean = false
        private set

    /** Called by the plugin, once Rust has registered it. */
    fun attachHost() {
        hostAttached = true
    }

    /** Leave a request for the host to collect. */
    fun deposit(request: Request) {
        depositedAt = SystemClock.elapsedRealtime()
        pending = request
    }

    /**
     * Take the pending request, clearing it.
     *
     * Cleared as it is read because there is exactly one consumer - the Rust
     * poll - and a broadcast must never be sent twice. A request older than
     * [FRESH_FOR_MS] is dropped: it was deposited in a process that had no host
     * to collect it, and sending it now would broadcast whatever was on the
     * clipboard back then.
     */
    @Synchronized
    fun take(): Request? {
        val request = pending ?: return null
        pending = null

        val age = SystemClock.elapsedRealtime() - depositedAt
        if (age > FRESH_FOR_MS) {
            Log.w(TAG, "dropping a broadcast request that waited ${age}ms")
            return null
        }

        return request
    }
}
