package app.cm.clipmesh.bridge.screenshot

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import androidx.core.content.ContextCompat

/**
 * The media-read permission the screenshot watcher needs, and nothing else.
 *
 * A `ContentObserver` on `MediaStore` fires without any permission at all - the
 * change notification is not the data - but opening the image does, so a watcher
 * without this permission would notice every screenshot and be able to read none
 * of them. That is the whole reason the feature is opt-in: it is the only part of
 * ClipMesh that asks to see the device's photo library.
 *
 * There are three states, not two, and the third one is the interesting one:
 *
 *  * **granted** - `READ_MEDIA_IMAGES` on Android 13+, `READ_EXTERNAL_STORAGE`
 *    below it. The whole library is readable.
 *  * **partial** - Android 14 and later let the user hand over *some* photos
 *    instead of all of them. `READ_MEDIA_VISUAL_USER_SELECTED` is granted and the
 *    real media permission is denied, and `MediaStore` then answers with only the
 *    selected items. A content observer under it still fires, so the feature
 *    would look alive while quietly missing most screenshots - which is why
 *    [ScreenshotWatcher] refuses to start in this state and the settings screen
 *    has to say so.
 *  * **denied** - nothing to do but tell the user.
 */
object MediaAccess {

    /**
     * Whether the *whole* photo library is readable.
     *
     * On Android 14+ this is deliberately false under partial access: the
     * question every caller is really asking is "will the watcher see the next
     * screenshot", not "did a permission dialog say yes".
     */
    fun granted(context: Context): Boolean = when {
        Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU ->
            holds(context, Manifest.permission.READ_MEDIA_IMAGES)

        else -> holds(context, Manifest.permission.READ_EXTERNAL_STORAGE)
    }

    /**
     * Whether the user granted access to a hand-picked subset instead.
     *
     * Only ever true on Android 14+; below that there is no such grant to hold.
     */
    fun partial(context: Context): Boolean =
        Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE &&
            !granted(context) &&
            holds(context, Manifest.permission.READ_MEDIA_VISUAL_USER_SELECTED)

    /**
     * The permission to request for this API level.
     *
     * `READ_MEDIA_IMAGES` is not a thing below Android 13, so asking for it
     * there would return "denied" without ever showing a dialog - which is how a
     * permission bug looks like a user refusing.
     */
    fun requestName(): String =
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            Manifest.permission.READ_MEDIA_IMAGES
        } else {
            Manifest.permission.READ_EXTERNAL_STORAGE
        }

    private fun holds(context: Context, permission: String): Boolean =
        ContextCompat.checkSelfPermission(context, permission) == PackageManager.PERMISSION_GRANTED
}
