# Maintenance notes

**English** | [简体中文](MAINTENANCE.zh-CN.md)

This document covers what a maintainer needs to know **before changing this code**: the `gen/android`
wiring that `tauri android init` does not reproduce, the Rust ↔ Kotlin bridge, the R8 keep rules,
edge-to-edge insets, where data is stored, and how to change the app icon.

Building and running it is in [`README.md`](../README.md).

---

## 1. Native plugin wiring (important)

The Kotlin plugin lives in `apps/android/plugins/bridge/` as a **standalone Gradle library module**,
outside `gen/android` — that way re-running `tauri android init` does not overwrite it.

Beyond the README's build steps, `gen/android` needs **four** hand-maintained places, all **already committed to the repo**; they are recorded here to explain why:

**① `apps/android/src-tauri/gen/android/settings.gradle`**

```gradle
include ':bridge'
project(':bridge').projectDir = new File(rootDir, '../../../plugins/bridge')
```

`rootDir` is `gen/android`, and it takes **three** levels of `..` to get back to `apps/android/`:

```
gen/android  --..-->  gen  --..-->  src-tauri  --..-->  android
```

> This was once written as two levels (`../../`), which is wrong — Gradle then cannot find the module and
> reports `Project with path ':bridge' could not be found`.

**② `apps/android/src-tauri/gen/android/app/build.gradle.kts`**

```kotlin
dependencies {
    implementation(project(":bridge"))
    ...
}
```

The plugin's `AndroidManifest.xml` uses the manifest merger to merge its permissions
(`FOREGROUND_SERVICE`, `FOREGROUND_SERVICE_DATA_SYNC`, `POST_NOTIFICATIONS`,
`RECEIVE_BOOT_COMPLETED`, `CHANGE_WIFI_MULTICAST_STATE`, plus `READ_MEDIA_IMAGES` /
`READ_MEDIA_VISUAL_USER_SELECTED` / `READ_EXTERNAL_STORAGE` for screenshot sync) and its
Service / Receiver / Activity / FileProvider declarations into the app.

**③ the same file — release signing**

The `signingConfigs` block and the `signingConfig = …` line inside `buildTypes.release`, described in
[`README.md`](../README.md) (Android → *Environment variables*).
Tauri's template ships neither, so a regenerated project produces unsigned release builds until they are put back.

**④ `apps/android/src-tauri/gen/android/app/src/main/java/app/cm/clipmesh/MainActivity.kt`**

The activity keeps the web content out from under the system bars, which nothing in the generated project does:
`enableEdgeToEdge()` makes the webview cover the whole display — and from `targetSdk = 35` the platform enforces
that whether or not the call is there — so `onWebViewCreate` pads the webview's parent content frame by
`systemBars() | displayCutout()`. See §4: the code is small, but it is the only thing standing between the
bottom tab bar and the navigation bar.

> If `gen/android` is regenerated (delete it, then run `tauri android init`), **all four places above** have
> to be added back. These are the only generated-file changes that need manual maintenance.
> (`apps/android/plugins/bridge/consumer-rules.pro` is deliberately **not** on the list: it contains R8 keep
> rules rather than wiring, lives in the plugin module outside `gen/android`, and reaches the app through
> `consumerProguardFiles` — see §3.)
>
> **The FileProvider used to be a fifth item here. It no longer is.** Sharing an image needs a `content://`
> URI, and `androidx.core.content.FileProvider` used to be declared in
> `gen/android/app/src/main/AndroidManifest.xml` with its paths in
> `gen/android/app/src/main/res/xml/file_paths.xml`. It now lives in the plugin's own manifest
> (`apps/android/plugins/bridge/src/main/AndroidManifest.xml`, same `${applicationId}.fileprovider`
> authority, paths in `bridge/src/main/res/xml/clipmesh_file_paths.xml`), so regenerating `gen/android`
> cannot lose it. All that is left in the generated manifest is a comment saying not to add it back:
> declaring the same provider twice merges the two elements, and two different values for the
> `FILE_PROVIDER_PATHS` meta-data make that merge **fail the build** rather than duplicate harmlessly.

---

## 2. How the frontend talks to Kotlin

```
Vue  invoke("send_clipboard")
  ↓
Rust  clipmesh_core::SyncManager → AndroidClipboardProvider
  ↓
Rust  KotlinClipboardHost → PluginHandle::run_mobile_plugin("readClipboard")
  ↓
Kotlin ClipMeshPlugin.readClipboard  → ClipboardManager
```

The reverse direction (the notification shade button):

```
Notification "Broadcast clipboard"
  ↓  PendingIntent → BroadcastActivity (transparent theme, exported=false, separate taskAffinity)
  ↓  On Android 10+ only a foreground app can read the clipboard, so a window that can take focus is needed
Kotlin BroadcastActivity.onWindowFocusChanged → ClipboardAccess.read
  ↓  Whatever was read (or "nothing was read") is handed to the process-level BroadcastHandoff
Rust  broadcast::spawn polls takeBroadcast (500ms while the foreground service is running, 5s otherwise)
  ↓
Rust  engine.send_explicit(content)  → peer
```

The notification button **no longer** opens a visible UI, but the Activity has three possible failure modes;
the first two fall back to the old behaviour (bring the real `MainActivity` to the foreground, then
`moveTaskToBack` once the broadcast is done):

| Failure | Symptom | Handling |
| --- | --- | --- |
| No window focus within 2 seconds | The transparent Activity has started | Fallback: `ACTION_BROADCAST` brings the app to the foreground, `visible=true` |
| Focus obtained but the clipboard reads no content | Same as above | Same as above |
| The notification's `PendingIntent` is blocked by the ROM (background Activity launch restrictions) | Nothing happens at all | **Undetectable and unfixable**: the app side gets no callback at all, so the user can only use the in-app button |

Rust finishes the fallback path: read the clipboard (retrying a few times; reading nothing right after the
window appears is normal), `send_explicit`, and once that succeeds `leaveApp()` to send the user back to
the app they came from.

`send_explicit` rather than `AndroidClipboardProvider::push`: `push` emits a "local clipboard
change", and the engine's `handle_local_change` drops it when `autoSync` is off — and that is exactly
the user who would press this button by hand.

**Screenshot sync travels the same road** (the settings switch, off by default):

```
A change in MediaStore
  ↓  ScreenshotWatcher's ContentObserver (process-wide, registered on the application context)
  ↓  only new images in a screenshot folder (Pictures/Screenshots, DCIM/Screenshots, …)
Kotlin reads the image on a background thread → BroadcastHandoff.deposit(Request.Screenshot)
  ↓  Rust broadcast::spawn polls takeBroadcast (the answer carries source = "screenshot")
Rust  engine.send_explicit(image)  → peers, and into the history
```

The one difference from the clipboard road is that it does **not** write into the clipboard cache
(`AndroidClipboardProvider::converted` rather than `stage`): a screenshot was never on the clipboard,
and caching it would make a later background read report the screenshot as "what you just copied".

Rust ↔ Kotlin method names correspond one-to-one, and **changing one side means changing the other** (it only errors at runtime):

| Rust (`plugin.rs`) | Kotlin (`ClipMeshPlugin.kt`) |
| --- | --- |
| `readClipboard` | `readClipboard` |
| `setText` | `setText` |
| `setImage` | `setImage` |
| `showReceived` | `showReceived` |
| `showReceivedImage` | `showReceivedImage` |
| `shareImage` | `shareImage` |
| `startService` / `stopService` / `serviceRunning` | same names |
| `requestNotificationPermission` | `requestNotificationPermission` |
| `screenshotPermission` | `screenshotPermission` |
| `setScreenshotSync` | `setScreenshotSync` |
| `leaveApp` | `leaveApp` |
| `takeBroadcast` | `takeBroadcast` |

The Kotlin class name and package name are in the
`ANDROID_PLUGIN_PACKAGE` / `ANDROID_PLUGIN_CLASS` constants in `apps/android/src-tauri/src/plugin.rs`.

---

## 3. Release builds: R8 and the plugin's keep rules

Release is minified (`optimization { enable = true }` in `app/build.gradle.kts`), and R8 cannot see the one
reflective path the plugin depends on: `Invoke.parseArgs(SetTextArgs::class.java)` deserialises a command's
payload with **Jackson**, over the class object it is handed. Before the keep rules existed a release build
renamed `SetTextArgs` to `d20` and stripped its constructor and setters, so **every `@Command` that takes
arguments** (`setText`, `setImage`, `showReceived`, `showReceivedImage`, `shareImage`, `setScreenshotSync`)
failed at runtime — while the debug build, which does not minify, worked:

```
Cannot construct instance of `d20` (no Creators, like default constructor, exist)
```

The rules live in **`apps/android/plugins/bridge/consumer-rules.pro`** and are attached with
`consumerProguardFiles("consumer-rules.pro")` in the plugin module's `defaultConfig`. That is the idiomatic
place for the declaration — AGP folds a library's consumer rules into every minified consumer of it, and the
app's own R8 configuration proves the mechanism (`build/outputs/mapping/*/configuration.txt` carries
"Local project :::tauri-android" and "…:::tauri-plugin-opener" sections), so nothing has to be added to
`gen/android/app/proguard-rules.pro`, where `tauri android init` would eventually overwrite it.

**Adding a parameterised `@Command` means adding its argument class to those rules.** The `@Command` methods
themselves are kept by `:tauri-android`'s own consumer rules, and manifest components (the activities, the
service, the receivers) by AGP's `aapt_rules.txt` — the argument classes were the only gap.

---

## 4. Edge-to-edge, the system bars, and `env(safe-area-inset-*)`

`targetSdk = 37` means the platform forces the activity edge-to-edge, so the webview is laid out over the whole
display and the bottom navigation bar overlays it. Android does **not** hand that inset to CSS: the WebView
fills `env(safe-area-inset-*)` in only for the display *cutout*, and only while it occupies the entire screen, so
on a phone without a notch every one of those values is `0px`. The inset therefore has to be a layout inset, and
it is applied in **④ `MainActivity.kt`** (`onWebViewCreate` pads the webview's parent content frame by
`systemBars() | displayCutout()`); see §1 for where that file is wired in.

Two consequences worth knowing before touching either side:

- `apps/android/ui`'s `MobileLayout.vue` must **not** also add `env(safe-area-inset-*)`. On a WebView that does
  report system bars the two would stack, and the layout would reserve the bar height twice.
- The padding goes on the *parent* rather than on the webview: padding the webview would leave its own bounds
  covering the bar strips, and the strip behind the status bar is not the app's to draw in — that window is
  above the app's and eats the touches. The strips fall back to the theme's DayNight `windowBackground`.

---

## 5. Where data is stored

| Platform | Directory |
| --- | --- |
| Windows | `%APPDATA%\ClipMesh\` |
| Linux | `~/.config/ClipMesh/` |
| macOS | `~/Library/Application Support/ClipMesh/` |
| Android | app-private directory |

| File | Contents |
| --- | --- |
| `identity.json` | deviceId, device name, creation time |
| `device.key` | Ed25519 private key (PKCS#8 DER, 0600 on unix) |
| `device.crt` | self-signed X.509 certificate (PEM) |
| `trusted_devices.json` | paired devices: certificate + fingerprint + public key |
| `settings.json` | user settings |
| `history.json` | persisted clipboard history (the capacity is `historyCapacity` in the settings) |
| `images/` | PNG pixels of history images, one file per entry (`images/<id>.png`) |

**Re-pairing**: just delete `trusted_devices.json` (or delete it on both devices).
**Full reset**: delete the whole directory — note that this generates a new identity and invalidates every old pairing.

---

## 6. Changing the app icon

The source image is at the repo root: `icon.png` (square; `tauri icon` requires ≥1024, currently 1254×1254).

After swapping the image, one command is enough:

```bash
npm run icons
```

It does four things (the script is at [`scripts/update-icons.mjs`](../scripts/update-icons.mjs)):

1. Runs `tauri icon` in `apps/desktop`, generating every desktop + mobile format
2. Syncs the flat formats to `apps/android/src-tauri/icons/`
3. Copies the adaptive icon (`mipmap-*` + `mipmap-anydpi-v26` + background colour) into
   `apps/android/src-tauri/gen/android/app/src/main/res/`
4. Deletes the extra `android/` and `ios/` subdirectories in the desktop project

### Background colour

The background layer of an Android adaptive icon is **a single flat colour**. `tauri icon` always writes `#FFFFFF`,
which leaves a white ring around a colourful icon, so the script rewrites this file:

```
apps/android/src-tauri/gen/android/app/src/main/res/values/ic_launcher_background.xml
```

The default `#AABFF5` was obtained by sampling and averaging the midpoints of the current source image's four edges.
After switching to an image with very different colours it should be resampled — take the pixels about 40px inward
from the midpoint of each of the top, bottom, left and right edges and average them:

```bash
node scripts/update-icons.mjs --bg '#RRGGBB'
```

> Note that `npm run icons -- --bg ...` **does not work**: npm takes `--bg` as its own argument and reports
> `EUNKNOWNCONFIG`. To see the output, run `node` directly with `--source` / `--bg`.

### About the alpha channel

The current source image **has** alpha, and its rounded corners are genuinely transparent, so the corners in
`.ico` / `.icns` are transparent as well.
(The previous source image had no alpha; its corners were painted dark pixels, which showed up as square corners
in the Windows taskbar — if the image is ever swapped back to one without alpha, that problem comes back.)

### ⚠️ Swapped the icon but the exe still has the old one?

This is a trap this project actually fell into, and the cause is not the icon cache but **cargo's build script cache**:

On Windows, `tauri-build` generates a `resource.rc` that points at
`icons/icon.ico` via an absolute path, and that file gets compiled into the exe. But it only emits
`cargo:rerun-if-changed` for `tauri.conf.json` and `capabilities/`, **never for the icons**.

And cargo's rule is: **as soon as a build script emits any `rerun-if-changed`, the fallback
"rerun when a file inside the package changes" no longer applies**, and from then on only that list counts.
So `icons/icon.ico` is completely invisible to cargo — build.rs never reruns,
that `resource.rc` pointing at the old icon just sits in the cache and gets linked into every new exe,
and **the build succeeds, the program runs fine, and the icon is quietly wrong**.

The fix is already in both `build.rs` files (`apps/*/src-tauri/build.rs`):
before calling `tauri_build::build()`, emit `cargo:rerun-if-changed` for every icon in
`bundle.icon`. **When you change `bundle.icon` in `tauri.conf.json`,
remember to update the `ICONS` list in `build.rs` to match.**

If you suspect you are hitting this, here is how to confirm what is actually embedded in the exe:

```powershell
Add-Type -AssemblyName System.Drawing
$ico = [System.Drawing.Icon]::ExtractAssociatedIcon("target\release\clipmesh-desktop.exe")
$ico.ToBitmap().Save("$env:TEMP\embedded.png")
```

(Note that `ExtractAssociatedIcon` goes through the shell API and will hit the icon cache.
To bypass the cache, copy the exe to a new file name first, then extract.)

Once you have confirmed it is the cache, force a clean rebuild:

```bash
cargo clean -p clipmesh-desktop
```

### After changing icons, rebuild — do not just look at the old exe

The desktop executable is built to `target/release/clipmesh-desktop.exe` under the repo root — the
workspace target directory, not `apps/desktop/src-tauri/target/`. An exe left over from an earlier
build still carries the old icon, so rebuild rather than double-clicking a stale one.
