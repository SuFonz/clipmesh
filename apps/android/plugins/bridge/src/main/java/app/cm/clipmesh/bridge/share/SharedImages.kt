package app.cm.clipmesh.bridge.share

import android.content.ClipData
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.util.Log
import androidx.core.content.FileProvider
import java.io.File
import java.io.FileOutputStream

/**
 * The files ClipMesh hands to Android's share sheet.
 *
 * ## Why files, and why here
 *
 * A share is `Intent.ACTION_SEND` with a `content://` URI: Android refuses a
 * `file://` path from one app to another (`FileUriExposedException` since Android
 * 7), and a `Bitmap` cannot travel in an `Intent` at all. So an image that is to
 * be shared has to exist as a file under a path the app's `FileProvider` exposes
 * - which is what the provider in this module's manifest is for, with
 * `res/xml/clipmesh_file_paths.xml` naming this directory.
 *
 * Everything lands in `cacheDir/share/`:
 *
 *  * `received-<entry id>.png` - the newest image a peer sent, staged when its
 *    notification is posted, so the notification's share action has a URI without
 *    needing the Rust host to be alive;
 *  * `history-<entry id>.png` - a history image the user asked to share.
 *
 * ## What is cleaned up, and when
 *
 * One received file at a time: posting a new image notification makes the previous
 * one unreachable (same notification id), so its file goes. History files are
 * pruned by age rather than replaced, because a chooser target may still be
 * reading the URI from the previous share - a fixed file name would let a second
 * share overwrite the image the user just picked.
 */
object SharedImages {

    private const val TAG = "ClipMeshShare"

    /** Directory under `cacheDir`, and the name `file_paths.xml` exposes. */
    const val DIRECTORY = "share"

    /** MIME type of everything ClipMesh stages. */
    const val MIME_PNG = "image/png"

    /**
     * How long a history image stays shareable.
     *
     * Long enough that no chooser can still be holding the URI, short enough that
     * the cache directory cannot grow into a second image library.
     */
    private const val HISTORY_TTL_MS = 60 * 60 * 1000L

    private const val RECEIVED_PREFIX = "received-"
    private const val HISTORY_PREFIX = "history-"

    /**
     * Write the image of a received entry and return its file.
     *
     * Replaces any previously staged received image: only one received
     * notification exists at a time, so the older URI is dead the moment this
     * runs.
     */
    fun stageReceived(context: Context, entryId: String, png: ByteArray): File? {
        val file = write(context, "$RECEIVED_PREFIX${safe(entryId)}.png", png) ?: return null
        prune(context, RECEIVED_PREFIX, keep = file)
        return file
    }

    /**
     * Write a history image and return its file, reusing an earlier copy.
     *
     * Reuse matters for the home page and the history list: the same entry is
     * shared over and over, and re-encoding a multi-megabyte PNG every time would
     * be work for nothing.
     */
    fun stageHistory(context: Context, entryId: String, png: ByteArray): File? {
        val name = "$HISTORY_PREFIX${safe(entryId)}.png"
        val existing = File(directory(context), name)
        if (existing.isFile && existing.length() > 0) {
            prune(context, HISTORY_PREFIX, keep = existing, maxAgeMs = HISTORY_TTL_MS)
            return existing
        }

        val file = write(context, name, png) ?: return null
        prune(context, HISTORY_PREFIX, keep = file, maxAgeMs = HISTORY_TTL_MS)
        return file
    }

    /** The staged received image of an entry, if that is still the one staged. */
    fun receivedIfStaged(context: Context, entryId: String): File? {
        val file = File(directory(context), "$RECEIVED_PREFIX${safe(entryId)}.png")
        return if (file.isFile && file.length() > 0) file else null
    }

    /** The `content://` URI of a staged file. */
    fun uri(context: Context, file: File): Uri =
        FileProvider.getUriForFile(context, "${context.packageName}.fileprovider", file)

    /**
     * The chooser intent for one staged image.
     *
     * The URI is attached twice on purpose - as the `EXTRA_STREAM` every receiver
     * reads, and as the intent's `ClipData`, which is what carries the read grant
     * to whichever app the user picks.
     */
    fun chooserFor(context: Context, file: File): Intent {
        val uri = uri(context, file)

        val send = Intent(Intent.ACTION_SEND).apply {
            type = MIME_PNG
            putExtra(Intent.EXTRA_STREAM, uri)
            clipData = ClipData.newUri(context.contentResolver, "ClipMesh", uri)
            addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        }

        return Intent.createChooser(send, null).apply {
            // Always a new task: this is started from a notification's
            // PendingIntent as often as from the app, and it must not land inside
            // whichever task happens to be in front.
            addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        }
    }

    /** Remove the staged received image, if there is one. */
    fun dropReceived(context: Context) {
        prune(context, RECEIVED_PREFIX, keep = null)
    }

    /** Where staged files live. */
    fun directory(context: Context): File = File(context.cacheDir, DIRECTORY)

    private fun write(context: Context, name: String, png: ByteArray): File? = try {
        val directory = directory(context)
        if (!directory.exists() && !directory.mkdirs()) {
            Log.w(TAG, "could not create ${directory.absolutePath}")
        }

        val file = File(directory, name)

        // Write and rename, like every other file ClipMesh keeps: a chooser can be
        // handed the URI while the next write is in flight, and half a PNG is not
        // a PNG.
        val temporary = File(directory, "$name.tmp")
        FileOutputStream(temporary).use { output -> output.write(png) }
        if (file.exists() && !file.delete()) {
            Log.w(TAG, "could not replace ${file.name}")
        }
        if (!temporary.renameTo(file)) {
            Log.w(TAG, "could not stage ${file.name}")
            temporary.delete()
            null
        } else {
            file
        }
    } catch (error: Exception) {
        Log.w(TAG, "could not stage an image for sharing", error)
        null
    }

    /**
     * Drop staged files other than `keep`.
     *
     * `maxAgeMs` of null means "everything but `keep`": that is the received file,
     * of which exactly one can be reachable. History files are removed once they
     * are old enough that no chooser can still be reading them.
     */
    private fun prune(context: Context, prefix: String, keep: File?, maxAgeMs: Long? = null) {
        val directory = directory(context)
        val files = directory.listFiles() ?: return
        val cutoff = maxAgeMs?.let { System.currentTimeMillis() - it }

        for (file in files) {
            if (!file.name.startsWith(prefix)) continue
            if (keep != null && file.absolutePath == keep.absolutePath) continue
            if (cutoff != null && file.lastModified() >= cutoff) continue

            if (!file.delete()) Log.d(TAG, "could not remove ${file.name}")
        }
    }

    /**
     * Make an entry id safe to use as a file name.
     *
     * Ids arrive from peers. Every id ClipMesh mints is a UUID, so this is a path
     * traversal guard as much as a naming rule - the same reasoning as the image
     * store's on the Rust side.
     */
    private fun safe(id: String): String {
        val cleaned = id.filter { it.isLetterOrDigit() || it == '-' || it == '_' }
        return cleaned.take(128).ifEmpty { "unknown" }
    }
}
