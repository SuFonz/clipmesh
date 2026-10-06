# ClipMesh

**English** | [简体中文](README.zh-CN.md)

<p align="center">
  <img src="icon.png" alt="ClipMesh" width="160" />
</p>

**Secure cross-device clipboard sync** — no central server, direct P2P, end-to-end TLS encryption.

---

## What this is

ClipMesh syncs the clipboard between **your own** devices: copy some text or a screenshot on your
laptop, your phone gets a notification right away, and one tap pastes it.

No account, no cloud, no relay server. Devices find each other on the local network over mDNS and
open a **TLS 1.3 mutual-authentication** connection directly; data only ever moves between the two
devices.

```
     ┌──────────┐        mDNS discovery        ┌──────────┐
     │  Laptop  │◄────────────────────────────►│  Phone   │
     │          │                              │          │
     │          │     TCP + TLS 1.3 (mTLS)     │          │
     │          │◄────────────────────────────►│          │
     └──────────┘ clipboard / images / pairing └──────────┘
                No server. No third party.
```

## Screenshots

> Screenshots are on the way.

<!--
  To add one, drop the image under docs/ and uncomment:

  <p align="center">
    <img src="docs/desktop-home.png" alt="Desktop - home" width="720" />
  </p>
-->


## Core features

| | |
| --- | --- |
| 🔒 **TLS 1.3 mutual authentication** | Every device holds its own Ed25519 key and self-signed certificate; all traffic is encrypted |
| 👤 **Explicit pairing + fingerprint check** | Discovery ≠ trust. You must compare certificate fingerprints on both devices and accept manually |
| 🖥 **Text and image sync** | Images travel as PNG binary chunks, **never base64** |
| 🔍 **Automatic LAN discovery** | mDNS (`_clipmesh._tcp.local.`), no configuration, no port forwarding |
| 📋 **Background clipboard watching** | The desktop side watches automatically; Android uses a foreground service plus a notification button |
| 🧩 **Platform-independent core** | `crates/**` (apart from the per-platform `crates/clipboard`) depends on neither Tauri / Vue / Windows API / Android API |
| 🔁 **Loop prevention** | UUID dedup + echo suppression, so two devices never bounce the same item back at each other |

## Security model (present in the first version, not "later")

| Threat | Countermeasure |
| --- | --- |
| LAN eavesdropping | TLS 1.3, all traffic encrypted |
| Man-in-the-middle replacing a device | Certificate fingerprint pinning + the signature is bound to the TLS channel (see below) |
| Unpaired device reading the clipboard | An unpaired device **can only** send pairing messages — `PairRequest`, or the `PairAccept` answering a request this device sent; every other message is dropped |
| Device identity spoofing | `Hello` must be signed with the private key matching the public key in the certificate |
| Replay / relay | The signed content includes the TLS exporter secret, so it is valid only on the current connection |
| Malicious oversized frame exhausting memory | The framing layer checks the 16 MiB limit before allocating |

The signed handshake content is `sha256("clipmesh-hello-v1" ‖ challenge ‖ TLS exporter secret)`.
Only the two ends of this TLS connection can compute the exporter secret, so an attacker **cannot
forward A's handshake message onto another connection** and impersonate A — that is the key
difference between ClipMesh and "bare TLS".

See [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md).

## Repository layout

```
clipmesh/
├── crates/                    # Rust core; only clipboard/ is platform-specific
│   ├── protocol/              # protobuf protocol, framing, payload models
│   ├── identity/              # Ed25519, self-signed certificates, fingerprints, trust store
│   ├── security/              # rustls config, certificate policy, channel-binding handshake
│   ├── core/                  # sync engine: Provider trait, dedup, device table, events
│   ├── network/               # mDNS + TCP + TLS sessions
│   └── clipboard/             # per-platform clipboard (Android goes through a Kotlin bridge)
├── packages/ui-core/          # frontend shared by both apps: types / store / API / common components
├── apps/
│   ├── desktop/{src-tauri,ui} # desktop Tauri host + desktop layout
│   └── android/
│       ├── src-tauri/         # mobile Tauri host (reuses the desktop command layer)
│       ├── ui/                # mobile layout
│       └── plugins/bridge/   # Kotlin plugin: clipboard / notifications / foreground service
└── docs/
```

## Quick start

### Requirements

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

> **What has actually been verified.** Only the **Windows** build environment has been exercised:
> building on Windows produces both a working Windows app and a working Android APK. **No other
> combination has been verified** — all three desktop platforms are wired up (see the per-platform
> bundle configs `apps/desktop/src-tauri/tauri.*.conf.json`) and the code that differs per platform is
> confined to `crates/clipboard`, but the Linux and macOS desktop builds have never been run. Treat
> those as untested rather than as known working.

### First-time install

```bash
npm install            # repo root, installs every workspace in one go
cargo fetch            # optional: warm the crates cache
```

> **Note**: the repo root has an `.npmrc` whose contents are `include=dev`.
> Some environments export `NODE_ENV=production`, which makes npm skip every devDependency by default,
> so the install finishes but the build cannot run (no vite / vue-tsc / tauri CLI).
> If you do not want that file, delete it and then use `npm install --include=dev`.

### Desktop

#### Development

```bash
npm run dev:desktop        # = tauri dev
```

Tauri will:
1. Run `beforeDevCommand` in `apps/desktop/`: `npm --prefix ui run dev` (Vite, port 1420, `strictPort`)
2. Compile `apps/desktop/src-tauri` (about 3–10 minutes the first time, incremental afterwards)
3. Open a window pointed at `http://localhost:1420`

#### Frontend only (no Rust compile)

```bash
npm run dev:desktop:ui
```

Open <http://localhost:1420> in a browser. When no Tauri runtime is detected
(`__TAURI_INTERNALS__` is absent), `packages/ui-core/src/api/transport.ts` switches to the built-in
mock backend by itself: three sample devices and history, with every interaction clickable and fake
events that change on their own. Good for UI work. See
[`packages/ui-core/src/api/mock.ts`](packages/ui-core/src/api/mock.ts).

#### Packaging

```bash
npm run build:desktop:ui     # ui/dist has to be generated first
npm run build:desktop        # = tauri build
```

The artifacts land in the workspace target directory at the repo root: `target/release/bundle/`.

> `tauri.conf.json`'s `frontendDist` is `../ui/dist`,
> i.e. `apps/desktop/ui/dist`. `tauri::generate_context!()` reads that directory at compile time,
> so **`cargo build` fails when the directory does not exist**. That is why, before a Rust-only
> `cargo check`, the frontend build has to run at least once. `dist/` is in `.gitignore`, so a fresh clone must build it itself.

#### Development logging

```bash
CLIPMESH_LOG=debug npm run dev:desktop        # Windows PowerShell: $env:CLIPMESH_LOG="debug"
```

### Android

The Gradle module wiring — the four places in `gen/android` that `tauri android init` does not
reproduce — is in [`docs/MAINTENANCE.md`](docs/MAINTENANCE.md) §1.

#### Environment variables

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

#### Rust target

```bash
rustup target add aarch64-linux-android armv7-linux-androideabi i686-linux-android x86_64-linux-android
```

#### Running and packaging

```bash
npm run dev:android              # = tauri android dev
npm run build:android:release    # = tauri android build  ->  signed APK / AAB
```

### Quality gate

Everything here should pass before committing:

```bash
cargo test --workspace                        # Rust: protocol / identity / security / engine / network / clipboard
cargo clippy --workspace --all-targets        # recommended
npm run typecheck                             # frontend type check
npm run build:ui                              # build the frontend for both apps
```

`cargo test --workspace` also compiles `apps/android/src-tauri`,
but it does **not** run Gradle — the Android Java/Kotlin side has to be verified on a real device or emulator,
or with `cd apps/android/src-tauri/gen/android && ./gradlew :bridge:assembleDebug`.

### Troubleshooting

| Symptom | Cause / what to do |
| --- | --- |
| ``The `frontendDist` configuration is set to `"../ui/dist"` but this path doesn't exist`` | Run `npm run build:desktop:ui` first |
| `Could not automatically determine the process-level CryptoProvider` | `rustls`'s `default-features` was turned on, which enables ring and aws-lc-rs at the same time. See **Requirements** above |
| Devices cannot discover each other | Check whether the firewall allows UDP 5353 (mDNS) and TCP 47711; some corporate Wi-Fi networks disable multicast |
| Port 47711 is already in use | Normal: it falls back to an ephemeral port and advertises the real port over mDNS |
| Still not syncing after pairing | Check `autoSync` / `syncText` / `syncImages` in the settings; make sure the other side shows as "online" in the device list |
| Android no longer syncs in the background | Check whether the foreground service is running (there is a switch on the settings page) and whether notification permission has been granted |
| Tapping "Broadcast clipboard" on Android jumps to the foreground | Expected behaviour, see [`docs/MAINTENANCE.md`](docs/MAINTENANCE.md) §2 |
| No vite after `npm install` | See the `.npmrc` note in **First-time install** above |
| Gradle cannot find `:bridge` | `gen/android` was regenerated; add all four places back as described in [`docs/MAINTENANCE.md`](docs/MAINTENANCE.md) §1 |
| A **release** build fails with ``Cannot construct instance of `d20` (no Creators, like default constructor, exist)`` when writing the clipboard (or the image, or showing the notification) | R8 stripped an argument class that `Invoke.parseArgs` deserialises by reflection. `apps/android/plugins/bridge/consumer-rules.pro` keeps them; if a new parameterised `@Command` was added, its argument class has to be added there too. See [`docs/MAINTENANCE.md`](docs/MAINTENANCE.md) §3 |
| Content sits under the status or navigation bar | The webview is not inset. `env(safe-area-inset-*)` cannot fix it on Android (see [`docs/MAINTENANCE.md`](docs/MAINTENANCE.md) §4) — check that place ④ from [`docs/MAINTENANCE.md`](docs/MAINTENANCE.md) §1 is still in `MainActivity.kt`, and that the mobile layout has not added `env()` back on top of it |
| `Error: The string "--" is not allowed in comments` (`mergeUniversalDebugResources`) | Two consecutive hyphens appear **inside a comment** in one of the `res/values/*.xml` files. The XML spec forbids that, and aapt2 only reports it during resource merging, at a position far from the real one. This repo hit it once: a comment in `ic_launcher_background.xml` contained `npm run icons -- --bg ...`. `scripts/update-icons.mjs` now has an assertion that stops this regression |
| `SigningConfig`/`compileSdk` mismatch | The plugin module's `compileSdk`/Java version must match `app/build.gradle.kts` (currently 37 / Java 8) |

## First run

1. Start ClipMesh on both devices.
2. Within a few seconds they discover each other (the other one shows up in the device list).
3. On either one, click "Pair".
4. **The other one shows a "Pairing requests" card in the Devices view, with the device name, platform and certificate fingerprint.**
5. Compare the fingerprints on both screens; if they match, click "Accept".

Comparing fingerprints is the real security boundary — a signature only proves "the other side holds
that device's private key", not "that device is the one on your desk".

## Platform differences

| | Desktop | Android |
| --- | --- | --- |
| Clipboard watching | Automatic polling (Windows uses the system sequence number, zero cost) | Android 10+ forbids background reads, so a notification button triggers it instead |
| Staying resident | System tray, closing the window does not quit | Foreground service + persistent notification |
| Receive alerts | In-app notification | System notification with a preview |
| Layout | Side navigation + multiple columns | Bottom tabs + single column + 48px touch targets |

Android's background clipboard restriction is an OS-level constraint: there is **no** legitimate way
for a background app to read the clipboard, so the "broadcast clipboard" button brings the app to the
foreground and reads it there. That is the design, not a compromise.

## Documentation

| Document | Contents |
| --- | --- |
| [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) | Architecture, module responsibilities, threat model, data flow |
| [`docs/IPC.md`](docs/IPC.md) | The frozen frontend ↔ Rust contract (commands, events, types) |
| [`docs/MAINTENANCE.md`](docs/MAINTENANCE.md) | Android plugin wiring, R8 keep rules, edge-to-edge insets, data locations, changing the app icon |

## Protocol version

The current `PROTOCOL_VERSION = 1`. On a version mismatch the connection is rejected and
`ERROR_CODE_PROTOCOL_VERSION_MISMATCH` is reported back, instead of attempting a compatible parse.

## License

MIT, full text in [`LICENSE`](LICENSE).
