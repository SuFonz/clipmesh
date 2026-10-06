# Building and Releasing

**English** | [简体中文](BUILD.zh-CN.md)

This document covers: desktop packaging, the Android build and native plugin wiring, and common troubleshooting.

---

## 1. Requirements

| Component | Version | Notes |
| --- | --- | --- |
| Rust | **1.85+** (edition 2024) | `rustup default stable` |
| Node.js | **20+** | Frontend and npm workspaces |
| JDK | **17+** | Android only |
| Android SDK | compileSdk **37** | Android only |
| Android NDK | **26+** | Android only |

**`protoc` is not needed.** Protocol compilation goes through `protox` (a pure Rust implementation),
so a freshly cloned machine can build with nothing but a Rust toolchain — the same holds for CI and cross-compilation hosts.

The crypto backend is pinned to **ring**: `rustls` and `tokio-rustls` both have their default features
explicitly disabled, so `aws-lc-rs` does not drag in a C toolchain / cmake / NASM. Do not turn their
`default-features` back on in `Cargo.toml` — that enables both providers at once, and
`ServerConfig::builder()` panics at runtime.

---

## 2. First-time install

```bash
npm install            # repo root, installs every workspace in one go
cargo fetch            # optional: warm the crates cache
```

> **Note**: the repo root has an `.npmrc` whose contents are `include=dev`.
> Some environments export `NODE_ENV=production`, which makes npm skip every devDependency by default,
> so the install finishes but the build cannot run (no vite / vue-tsc / tauri CLI).
> If you do not want that file, delete it and then use `npm install --include=dev`.

---

## 3. Desktop

### Development

```bash
npm run dev:desktop        # = tauri dev
```

Tauri will:
1. Run `beforeDevCommand` in `apps/desktop/`: `npm --prefix ui run dev` (Vite, port 1420, `strictPort`)
2. Compile `apps/desktop/src-tauri` (about 3–10 minutes the first time, incremental afterwards)
3. Open a window pointed at `http://localhost:1420`

### Frontend only (no Rust compile)

```bash
npm run dev:desktop:ui
```

Open <http://localhost:1420> in a browser. When `__TAURI_INTERNALS__` is not detected,
`packages/ui-core/src/api/transport.ts` falls back to `api/mock.ts` automatically,
serving three sample devices, history and fake events that change on their own. Good for UI work.

### Packaging

```bash
npm run build:desktop:ui     # ui/dist has to be generated first
npm run build:desktop        # = tauri build
```

The artifacts land in the workspace target directory at the repo root: `target/release/bundle/`.

> `tauri.conf.json`'s `frontendDist` is `../ui/dist`,
> i.e. `apps/desktop/ui/dist`. `tauri::generate_context!()` reads that directory at compile time,
> so **`cargo build` fails when the directory does not exist**. That is why, before a Rust-only
> `cargo check`, the frontend build has to run at least once. `dist/` is in `.gitignore`, so a fresh clone must build it itself.

### Development logging

```bash
CLIPMESH_LOG=debug npm run dev:desktop        # Windows PowerShell: $env:CLIPMESH_LOG="debug"
```

---

## 4. Android

### 4.1 Environment variables

```powershell
$env:JAVA_HOME      = "D:\Program Files\Java\jdk-17"      # or the jbr bundled with Android Studio
$env:ANDROID_HOME   = "D:\Program Files\Android\Sdk"
$env:NDK_HOME       = "$env:ANDROID_HOME\ndk\29.0.13846066"
```

Tauri also reads `TAURI_ANDROID_PROJECT_PATH` (defaults to `src-tauri/gen/android`).

**Release signing.** `app/build.gradle.kts` takes the signing material from four values read *outside* the repository — a Gradle property first, then the environment variable. Putting them in the **global** `~/.gradle/gradle.properties` keeps them out of this project entirely, and out of `git status`:

```properties
KEYSTORE_FILE=C:\\path\\to\\store.keystore
KEYSTORE_PASSWORD=…
KEY_ALIAS=…
KEY_PASSWORD=…
```

| Variable | Meaning |
| --- | --- |
| `KEYSTORE_FILE` | path to the `.jks` / `.keystore` — absolute, or relative to `app/` |
| `KEYSTORE_PASSWORD` | keystore password |
| `KEY_ALIAS` | the key's alias inside the keystore |
| `KEY_PASSWORD` | that key's password |

**Only release builds need them.** Debug builds — `npm run dev:android`, `assembleDebug` — sign with the debug key and ignore all four. If any of them is missing for a release build, the build still configures and completes, but the APK/AAB comes out **unsigned and cannot be installed**; Gradle prints a warning naming the ones that are missing.

> Android refuses to install an update whose signing key differs from the installed app's. Working around that means uninstalling first — and uninstalling deletes the app's private directory, which is where this device's identity lives. Every existing pairing is invalidated and has to be redone. Keep the release keystore, and back it up.

### 4.2 Rust target

```bash
rustup target add aarch64-linux-android armv7-linux-androideabi i686-linux-android x86_64-linux-android
```

### 4.3 Native plugin wiring (important)

The Kotlin plugin lives in `apps/android/plugins/bridge/` as a **standalone Gradle library module**,
outside `gen/android` — that way re-running `tauri android init` does not overwrite it.

Beyond the README, there are only three wiring points, all **already committed to the repo**; they are recorded here to explain why:

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
`RECEIVE_BOOT_COMPLETED`, `CHANGE_WIFI_MULTICAST_STATE`) and its Service / Receiver declarations into the app.

**③ the same file — release signing**

The `signingConfigs` block and the `signingConfig = …` line inside `buildTypes.release`, described in §4.1.
Tauri's template ships neither, so a regenerated project produces unsigned release builds until they are put back.

> If `gen/android` is regenerated (delete it, then run `tauri android init`), **all three places above** have
> to be added back. These are the only generated-file changes that need manual maintenance.

### 4.4 Running and packaging

```bash
npm run dev:android              # = tauri android dev
npm run build:android:release    # = tauri android build  ->  signed APK / AAB
```

### 4.5 How the frontend talks to Kotlin

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

Rust ↔ Kotlin method names correspond one-to-one, and **changing one side means changing the other** (it only errors at runtime):

| Rust (`plugin.rs`) | Kotlin (`ClipMeshPlugin.kt`) |
| --- | --- |
| `readClipboard` | `readClipboard` |
| `setText` | `setText` |
| `setImage` | `setImage` |
| `showReceived` | `showReceived` |
| `startService` / `stopService` / `serviceRunning` | same names |
| `requestNotificationPermission` | `requestNotificationPermission` |
| `leaveApp` | `leaveApp` |
| `takeBroadcast` | `takeBroadcast` |

The Kotlin class name and package name are in the
`ANDROID_PLUGIN_PACKAGE` / `ANDROID_PLUGIN_CLASS` constants in `apps/android/src-tauri/src/plugin.rs`.

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

## 6. Troubleshooting

| Symptom | Cause / what to do |
| --- | --- |
| ``The `frontendDist` configuration is set to `"../ui/dist"` but this path doesn't exist`` | Run `npm run build:desktop:ui` first |
| `Could not automatically determine the process-level CryptoProvider` | `rustls`'s `default-features` was turned on, which enables ring and aws-lc-rs at the same time. See §1 |
| Devices cannot discover each other | Check whether the firewall allows UDP 5353 (mDNS) and TCP 47711; some corporate Wi-Fi networks disable multicast |
| Port 47711 is already in use | Normal: it falls back to an ephemeral port and advertises the real port over mDNS |
| Still not syncing after pairing | Check `autoSync` / `syncText` / `syncImages` in the settings; make sure the other side shows as "online" in the device list |
| Android no longer syncs in the background | Check whether the foreground service is running (there is a switch on the settings page) and whether notification permission has been granted |
| Tapping "Broadcast clipboard" on Android jumps to the foreground | Expected behaviour, see §4.5 |
| No vite after `npm install` | See the `.npmrc` note in §2 |
| Gradle cannot find `:bridge` | `gen/android` was regenerated; add all three wiring points back as described in §4.3 |
| `Error: The string "--" is not allowed in comments` (`mergeUniversalDebugResources`) | Two consecutive hyphens appear **inside a comment** in one of the `res/values/*.xml` files. The XML spec forbids that, and aapt2 only reports it during resource merging, at a position far from the real one. This repo hit it once: a comment in `ic_launcher_background.xml` contained `npm run icons -- --bg ...`. `scripts/update-icons.mjs` now has an assertion that stops this regression |
| `SigningConfig`/`compileSdk` mismatch | The plugin module's `compileSdk`/Java version must match `app/build.gradle.kts` (currently 37 / Java 8) |

---

## 7. Changing the app icon

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

---

## 8. Quality gate

Everything here should pass before committing:

```bash
cargo test --workspace                        # Rust unit tests
cargo clippy --workspace --all-targets        # recommended
npm run typecheck                             # frontend type check
npm run build:ui                              # build both frontends
```

`cargo test --workspace` also compiles `apps/android/src-tauri`,
but it does **not** run Gradle — the Android Java/Kotlin side has to be verified on a real device or emulator,
or with `cd apps/android/src-tauri/gen/android && ./gradlew :bridge:assembleDebug`.
