package app.cm.clipmesh.bridge.clipboard

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.os.Build
import android.util.Base64
import android.util.Log

/**
 * Everything that touches the Android clipboard.
 *
 * Two platform rules shape this class:
 *
 *  * **Main thread.** [ClipboardManager] must be used from the thread that owns
 *    the window. Callers hop to the main looper; this class does not, so it can
 *    be tested without a `Looper`.
 *  * **Background reads are forbidden.** Since Android 10, `getPrimaryClip`
 *    returns null for a background app. There is no polling equivalent of the
 *    desktop watcher, which is why broadcasting is triggered by an explicit
 *    notification action rather than by noticing a change.
 */
object ClipboardAccess {

    private const val TAG = "ClipMeshClipboard"

    /** What a read produced. */
    sealed class Content {
        /** Plain text. */
        data class Text(val text: String) : Content()

        /** A decoded image, ready to be re-encoded as PNG by the caller. */
        data class Image(val bitmap: Bitmap) : Content()

        /** Nothing ClipMesh carries. */
        object Empty : Content()
    }

    private fun manager(context: Context): ClipboardManager? =
        context.getSystemService(Context.CLIPBOARD_SERVICE) as? ClipboardManager

    /**
     * Read the current clipboard.
     *
     * Returns [Content.Empty] rather than throwing when Android refuses the read
     * because the app is in the background - that is an expected state, not an
     * error, and the user simply gets no broadcast.
     */
    fun read(context: Context): Content {
        val clipboard = manager(context) ?: return Content.Empty

        val clip: ClipData = try {
            clipboard.primaryClip ?: return Content.Empty
        } catch (error: SecurityException) {
            // Thrown when the app does not have focus on newer releases.
            Log.i(TAG, "the clipboard is not readable while in the background", error)
            return Content.Empty
        }

        if (clip.itemCount == 0) return Content.Empty
        val item = clip.getItemAt(0)

        // Text first, for the same reason the desktop provider prefers it: a
        // copied selection often carries both, and the text is what was meant.
        val text = item.coerceToText(context)?.toString()
        if (!text.isNullOrEmpty()) return Content.Text(text)

        val uri = item.uri
        if (uri != null) {
            return try {
                context.contentResolver.openInputStream(uri)?.use { stream ->
                    val bitmap = BitmapFactory.decodeStream(stream)
                    if (bitmap != null) Content.Image(bitmap) else Content.Empty
                } ?: Content.Empty
            } catch (error: Exception) {
                Log.w(TAG, "could not read an image from the clipboard", error)
                Content.Empty
            }
        }

        return Content.Empty
    }

    /** Put text on the clipboard. */
    fun writeText(context: Context, text: String): Boolean {
        val clipboard = manager(context) ?: return false
        return try {
            clipboard.setPrimaryClip(ClipData.newPlainText("ClipMesh", text))
            true
        } catch (error: Exception) {
            Log.w(TAG, "could not write text to the clipboard", error)
            false
        }
    }

    /** Put an image on the clipboard. */
    fun writeImage(context: Context, bitmap: Bitmap): Boolean {
        val clipboard = manager(context) ?: return false
        return try {
            clipboard.setPrimaryClip(ClipData.newPlainText("ClipMesh", ""))
            // `ClipData.newPlainText` cannot hold a bitmap, and there is no
            // public API to put one there. Android's own "copy image" uses a
            // content URI, which requires a FileProvider; until that is wired up
            // the image is written through the media store instead.
            Log.i(TAG, "image clipboard writes go through MediaStore on this API level")
            false
        } catch (error: Exception) {
            Log.w(TAG, "could not write an image to the clipboard", error)
            false
        }
    }

    /**
     * Whether the clipboard can be read right now.
     *
     * Android 10 and later only allow reads from a focused app; below that
     * (API 24-28) background reads still work, which is why the minimum SDK is
     * 24.
     */
    fun canReadInBackground(): Boolean = Build.VERSION.SDK_INT < Build.VERSION_CODES.Q

    /** Decode a base64 PNG sent from Rust. */
    fun decodeBase64Png(encoded: String): Bitmap? = try {
        val bytes = Base64.decode(encoded, Base64.DEFAULT)
        BitmapFactory.decodeByteArray(bytes, 0, bytes.size)
    } catch (error: Exception) {
        Log.w(TAG, "could not decode an image from the plugin", error)
        null
    }
}
