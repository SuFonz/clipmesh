package app.cm.clipmesh.bridge.screenshot

import android.content.ContentResolver
import android.content.ContentUris
import android.content.Context
import android.database.ContentObserver
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.net.Uri
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.provider.MediaStore
import android.util.Log
import app.cm.clipmesh.bridge.broadcast.BroadcastHandoff
import app.cm.clipmesh.bridge.clipboard.ClipboardAccess
import java.util.concurrent.Executor
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicBoolean

/**
 * Notices screenshots and hands them to the host, which broadcasts them.
 *
 * ## How a screenshot is recognised
 *
 * `MediaStore` has no "this is a screenshot" flag, so the platform's own
 * convention is the only signal there is: **where the file was written**. Every
 * Android build and every OEM skin drops screenshots into a folder called
 * *Screenshots* - `Pictures/Screenshots` on AOSP and most skins,
 * `DCIM/Screenshots` on Samsung and a few others - so the check is the folder
 * name at any depth of the relative path, plus the media store's own
 * `BUCKET_DISPLAY_NAME`, which is that same folder name for images indexed by
 * folder. Nothing else is consulted: treating every new image as a screenshot
 * would broadcast the user's entire camera roll, and a *file name* check
 * (`Screenshot_…`) was deliberately left out as well, because it matches a
 * downloaded or restored file just as happily.
 *
 * ## Where it lives, and how long
 *
 * The observer is registered against the **application context** and held in a
 * process-wide singleton rather than in the plugin instance, which belongs to the
 * activity: the whole point is to notice a screenshot while the user is in some
 * other app, with ClipMesh in the background behind its foreground service and no
 * activity alive at all. It therefore dies with the process, and nothing is
 * watched while the process is dead - which is consistent, because the foreground
 * service is what keeps both the process and the sync engine alive.
 *
 * The work happens on a background thread: a full-resolution screenshot is a few
 * megabytes of pixels to decode and re-encode, and doing that on the main looper
 * would stutter whatever the user is looking at.
 */
object ScreenshotWatcher {

    private val lock = Any()

    /** Serialises scans: one observer callback per row, and rows come in bursts. */
    private val worker = Executors.newSingleThreadExecutor { runnable ->
        Thread(runnable, "clipmesh-screenshots")
    }

    private val scanning = AtomicBoolean(false)

    private var observer: Observer? = null

    /** Whether an observer is registered right now. */
    val isWatching: Boolean
        get() = synchronized(lock) { observer != null }

    /**
     * Start watching, if the media permission allows it.
     *
     * Returns whether the observer is registered afterwards. A missing - or
     * partial - permission is not an error here: it is the state the settings
     * screen reports, and the reason this returns a boolean instead of throwing.
     *
     * Must be called on the main thread: `onChange` is delivered on the looper the
     * observer was created on, and everything downstream (`BroadcastHandoff`, the
     * plugin's clipboard work) already assumes the main thread. The plugin's
     * `onMain` is what guarantees it.
     */
    fun start(context: Context): Boolean {
        val app = context.applicationContext

        synchronized(lock) {
            if (observer != null) return true

            if (!MediaAccess.granted(app)) {
                Log.i(
                    TAG,
                    "not watching: the media permission is missing" +
                        if (MediaAccess.partial(app)) " (only selected photos are readable)" else "",
                )
                return false
            }

            val instance = Observer(app, worker, scanning)

            return try {
                // Where the library ends *before* the observer exists: an image
                // inserted while registering is still announced afterwards, while
                // everything already there is history rather than news. Priming
                // after registering would race the other way and could drop a
                // screenshot taken in the same millisecond.
                instance.markCurrentPosition()

                app.contentResolver.registerContentObserver(
                    MediaStore.Images.Media.EXTERNAL_CONTENT_URI,
                    // Descendants too: an insert is announced for the item's own
                    // URI as often as for the collection.
                    true,
                    instance,
                )

                observer = instance
                Log.i(TAG, "watching for screenshots")
                true
            } catch (error: Exception) {
                Log.w(TAG, "could not register the screenshot observer", error)
                false
            }
        }
    }

    /** Stop watching. Safe to call when nothing is registered. */
    fun stop() {
        val instance = synchronized(lock) {
            val current = observer ?: return
            observer = null
            current
        }

        try {
            instance.context.contentResolver.unregisterContentObserver(instance)
            Log.i(TAG, "stopped watching for screenshots")
        } catch (error: Exception) {
            Log.w(TAG, "could not unregister the screenshot observer", error)
        }
    }
}

private const val TAG = "ClipMeshScreenshots"

/**
 * Folder names that mean "the platform puts screenshots here".
 *
 * Compared case-insensitively against one path segment or the bucket name. The
 * non-English entries are not guesses about translation - most skins keep the
 * English folder name even in a Chinese or Japanese locale - but a few regional
 * builds and third-party galleries do localise it, and a name in this list costs
 * nothing while a missing one costs the feature.
 */
private val SCREENSHOT_FOLDERS = setOf(
    "screenshot",
    "screenshots",
    "screen shots",
    "screen_shots",
    "截屏",
    "截图",
    "スクリーンショット",
    "스크린샷",
)

/**
 * Decode ceiling, as an out-of-memory guard rather than a policy.
 *
 * A screenshot is normally far under this. The guard exists because
 * `BitmapFactory` on a huge import would allocate hundreds of megabytes and take
 * the process down, and a downscaled broadcast is a much better failure than a
 * crash.
 */
private const val MAX_DECODE_DIMENSION = 8_192
private const val MAX_DECODE_PIXELS = 32L * 1024 * 1024

/**
 * The observer itself.
 *
 * It owns the scan and the id it has already seen; the executor and the "a scan
 * is running" flag are handed in rather than reached for, so this class needs no
 * access to the singleton that created it.
 *
 * `onChange` runs on the main looper, so it does no work beyond queueing a scan.
 * The `Handler` is the deprecated form of the `ContentObserver` constructor and
 * is used deliberately: the newer one delivers on whichever binder thread the
 * media provider happened to use.
 */
private class Observer(
    val context: Context,
    private val worker: Executor,
    private val scanning: AtomicBoolean,
) : ContentObserver(Handler(Looper.getMainLooper())) {

    /** Highest `_ID` already accounted for. */
    @Volatile
    private var lastSeenId: Long = 0

    /** Record where the library ends, so only later inserts count. */
    fun markCurrentPosition() {
        lastSeenId = queryNewestId(context.contentResolver)
    }

    override fun onChange(selfChange: Boolean, uri: Uri?) {
        // The uri is not used: it is the item's own uri for an insert and the
        // collection's for a bulk change, and querying by id handles both.
        scan()
    }

    private fun scan() {
        if (!scanning.compareAndSet(false, true)) return

        worker.execute {
            try {
                collect()
            } catch (error: Exception) {
                // A media store that refuses a query is not worth a crash: the
                // next screenshot tries again.
                Log.w(TAG, "could not scan for new screenshots", error)
            } finally {
                scanning.set(false)
            }
        }
    }

    private fun collect() {
        val rows = queryNewImages(context.contentResolver, lastSeenId)
        if (rows.isEmpty()) return

        // Advanced for every row, screenshots included, before any of them is
        // read: a read that fails must not make the same row look new on the
        // next callback.
        lastSeenId = maxOf(lastSeenId, rows.maxOf { it.id })

        // Oldest first, so that if two screenshots land in one batch the handoff
        // ends up holding the newer one.
        for (row in rows.sortedBy { it.id }) {
            if (!looksLikeScreenshot(row.relativePath, row.bucket)) {
                Log.d(TAG, "ignoring a new image in ${row.relativePath ?: row.bucket ?: "?"}")
                continue
            }

            val content = readImage(context, row.uri)
            if (content == null) {
                Log.w(TAG, "could not read screenshot ${row.id}")
                continue
            }

            Log.i(TAG, "screenshot ${row.id} from ${row.relativePath ?: row.bucket}")
            BroadcastHandoff.deposit(BroadcastHandoff.Request.Screenshot(content))
        }
    }
}

/** One row of the images collection, reduced to what the filter needs. */
private data class Row(
    val id: Long,
    val uri: Uri,
    val relativePath: String?,
    val bucket: String?,
)

/**
 * Whether a folder name is one the platform uses for screenshots.
 *
 * `relativePath` wins when present (Android 10+), because it is the folder the
 * file is actually in; `bucket` is the media store's own grouping and is the only
 * signal available below Android 10. Both are checked as *whole* segments, so
 * `Pictures/MyScreenshots` is not a screenshot folder.
 */
internal fun looksLikeScreenshot(relativePath: String?, bucket: String?): Boolean {
    if (bucket != null && isScreenshotFolder(bucket)) return true
    val path = relativePath ?: return false
    return path.split('/').any { it.isNotEmpty() && isScreenshotFolder(it) }
}

private fun isScreenshotFolder(name: String): Boolean =
    SCREENSHOT_FOLDERS.contains(name.trim().lowercase())

/**
 * Every image added after `afterId`, oldest first.
 *
 * `RELATIVE_PATH` only exists from Android 10; below that the absolute `DATA`
 * path is the only way to see the folder, and it is still returned there.
 */
private fun queryNewImages(resolver: ContentResolver, afterId: Long): List<Row> {
    val collection = MediaStore.Images.Media.EXTERNAL_CONTENT_URI
    val modern = Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q
    val pathColumnName = if (modern) {
        MediaStore.Images.Media.RELATIVE_PATH
    } else {
        MediaStore.Images.Media.DATA
    }

    val projection = arrayOf(
        MediaStore.Images.Media._ID,
        pathColumnName,
        MediaStore.Images.Media.BUCKET_DISPLAY_NAME,
    )

    val rows = mutableListOf<Row>()
    resolver.query(
        collection,
        projection,
        "${MediaStore.Images.Media._ID} > ?",
        arrayOf(afterId.toString()),
        "${MediaStore.Images.Media._ID} ASC",
    )?.use { cursor ->
        val idColumn = cursor.getColumnIndexOrThrow(MediaStore.Images.Media._ID)
        val pathColumn = cursor.getColumnIndex(pathColumnName)
        val bucketColumn = cursor.getColumnIndex(MediaStore.Images.Media.BUCKET_DISPLAY_NAME)

        while (cursor.moveToNext()) {
            val id = cursor.getLong(idColumn)
            rows += Row(
                id = id,
                uri = ContentUris.withAppendedId(collection, id),
                relativePath = pathColumn.takeIf { it >= 0 }?.let { cursor.getString(it) },
                bucket = bucketColumn.takeIf { it >= 0 }?.let { cursor.getString(it) },
            )
        }
    }

    return rows
}

/** The highest id in the collection, or 0 when it is empty. */
private fun queryNewestId(resolver: ContentResolver): Long {
    resolver.query(
        MediaStore.Images.Media.EXTERNAL_CONTENT_URI,
        arrayOf(MediaStore.Images.Media._ID),
        null,
        null,
        "${MediaStore.Images.Media._ID} DESC",
    )?.use { cursor ->
        if (cursor.moveToFirst()) return cursor.getLong(0)
    }

    return 0
}

/**
 * Read one image out of the media store.
 *
 * Returns the same shape the clipboard path produces, so the host treats a
 * screenshot exactly like a copied image - which is what "broadcast it through
 * the normal sync path" means.
 */
private fun readImage(context: Context, uri: Uri): ClipboardAccess.Content.Image? {
    val resolver = context.contentResolver

    // Bounds first: decoding a full-resolution image only to shrink it wastes the
    // memory the shrinking was meant to save.
    val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
    try {
        resolver.openInputStream(uri)?.use { BitmapFactory.decodeStream(it, null, bounds) }
    } catch (error: Exception) {
        Log.w(TAG, "could not measure an image", error)
        return null
    }

    if (bounds.outWidth <= 0 || bounds.outHeight <= 0) return null

    val options = BitmapFactory.Options().apply {
        inSampleSize = sampleSizeFor(bounds.outWidth, bounds.outHeight)
    }

    val bitmap: Bitmap = try {
        resolver.openInputStream(uri)?.use { BitmapFactory.decodeStream(it, null, options) }
            ?: return null
    } catch (error: SecurityException) {
        // The permission was revoked between the change and the read.
        Log.w(TAG, "the media permission no longer covers this image", error)
        return null
    } catch (error: Exception) {
        Log.w(TAG, "could not decode an image", error)
        return null
    } ?: return null

    return ClipboardAccess.Content.Image(bitmap)
}

/**
 * The power-of-two subsampling factor that brings an image under the ceiling.
 *
 * Returns 1 for anything normal, which is every screenshot a phone produces.
 */
internal fun sampleSizeFor(width: Int, height: Int): Int {
    var sample = 1
    while (
        width / sample > MAX_DECODE_DIMENSION ||
        height / sample > MAX_DECODE_DIMENSION ||
        (width / sample).toLong() * (height / sample) > MAX_DECODE_PIXELS
    ) {
        sample *= 2
    }
    return sample
}
