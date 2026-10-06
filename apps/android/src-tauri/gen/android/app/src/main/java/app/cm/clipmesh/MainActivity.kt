package app.cm.clipmesh

import android.os.Bundle
import android.util.Log
import android.view.View
import android.webkit.WebView
import androidx.activity.enableEdgeToEdge
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat

/**
 * Keeps the web content out from under the system bars.
 *
 * WHY THIS EXISTS
 * ---------------
 * `enableEdgeToEdge()` - and, from `targetSdk = 35`, the platform itself - lays the
 * webview over the whole display, and nothing was insetting it: wry installs it as the
 * activity's content view (`wry/src/android/main_pipe.rs` -> `Activity.setContentView`),
 * so it is a MATCH_PARENT child of the content frame with no padding of its own. The
 * bottom tab bar therefore sat *behind* the navigation bar. The top edge had the same
 * overlap, and there it is worse than cosmetic: the status bar window is above the app
 * window in the z-order and swallows touches in its strip, so anything drawn there is
 * unreachable rather than merely hidden.
 *
 * WHY THE FRONTEND CANNOT DO THIS
 * -------------------------------
 * `env(safe-area-inset-*)` cannot express it on Android: the WebView only fills those in
 * for the display *cutout*, and only while it occupies the entire screen
 * (`AwDisplayCutoutController` / `AwDisplayModeController` in Chromium). The status and
 * navigation bars are never reported to CSS, so the `env()` rules that used to be in the
 * mobile layout resolved to 0 here. This also means the layout must NOT add `env()` back
 * on top of the padding below - see the note in `apps/android/ui/src/layouts/MobileLayout.vue`.
 *
 * WHY THE CONTENT FRAME RATHER THAN THE WEBVIEW
 * ---------------------------------------------
 * Padding the webview itself insets the page but leaves the webview's own bounds covering
 * the bar strips - the bar that takes touches would still be an app view - and its padding
 * band is painted by the webview, which has no background colour configured in
 * `tauri.conf.json`. Padding its parent shrinks the MATCH_PARENT webview inside it, so the
 * strips fall back to the theme's DayNight `windowBackground`, which is the colour
 * `enableEdgeToEdge()` already tints the bar icons against.
 *
 * WHY THE LISTENER GOES ON THAT PARENT, AND WHY THE DISPATCH BELOW IS REPEATED
 * ---------------------------------------------------------------------------
 * A listener replaces `View#onApplyWindowInsets` for the view it is set on *and for its
 * whole subtree* - `View#dispatchApplyWindowInsets` returns the listener's result and never
 * reaches the children. Chromium wants these insets for two things of its own: the
 * `env(safe-area-inset-*)` values it does support, and the visual viewport it shrinks when
 * the keyboard covers the page. So the listener is set on the parent - a view nothing else
 * claims - and the dispatch it suppresses is repeated by hand for the webview.
 *
 * (Which is also why the listener cannot simply be set on the webview and forward to it:
 * forwarding means calling `dispatchApplyWindowInsets` on the webview, and a listener on
 * the webview would make that call re-enter the listener.)
 */
class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
  }

  /**
   * The webview is handed to us before `setContentView` runs, so it has no parent yet.
   * The listener is installed from the attach callback instead - which fires before the
   * first insets dispatch, since insets are only ever dispatched to an attached view.
   */
  override fun onWebViewCreate(webView: WebView) {
    webView.addOnAttachStateChangeListener(
      object : View.OnAttachStateChangeListener {
        override fun onViewAttachedToWindow(view: View) {
          val host = view.parent as? View
          if (host == null) {
            // Unreachable: an attached view has a parent. Refuse rather than fall back
            // to padding the webview itself - a wrong layer here still looks right in a
            // screenshot while leaving the bar strips inside the app. Logged rather than
            // thrown because this runs in a user's app.
            Log.w(
              TAG,
              "window insets reached a webview with no parent; " +
                "the app will draw under the system bars",
            )
            return
          }

          ViewCompat.setOnApplyWindowInsetsListener(host) { _, insets ->
            // systemBars() is the status and navigation bars; displayCutout() is unioned
            // in so a notch taller than the status bar still clears. getInsets takes the
            // per-edge maximum of the types given to it, which is what we want.
            val bars = insets.getInsets(
              WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout(),
            )

            // The equality check is what terminates the padding -> layout -> insets ->
            // padding loop; it is not an optimisation.
            if (
              host.paddingLeft != bars.left ||
              host.paddingTop != bars.top ||
              host.paddingRight != bars.right ||
              host.paddingBottom != bars.bottom
            ) {
              host.setPadding(bars.left, bars.top, bars.right, bars.bottom)
            }

            // The dispatch the listener above suppressed; see the class comment.
            insets.toWindowInsets()?.let { webView.dispatchApplyWindowInsets(it) }

            // Left unconsumed: nothing else in this activity reads insets, and consuming
            // them would silently break whatever is added to the content frame next.
            insets
          }

          // The attach pass may have dispatched insets before this listener existed.
          ViewCompat.requestApplyInsets(host)
        }

        override fun onViewDetachedFromWindow(view: View) = Unit
      },
    )
  }

  companion object {
    /** Logcat tag: `adb logcat -s ClipMeshInsets`. */
    private const val TAG = "ClipMeshInsets"
  }
}
