# ClipMesh

**English** | [简体中文](README.zh-CN.md)

**Secure cross-device clipboard sync** — no central server, direct P2P, end-to-end TLS encryption.

Windows · Linux · macOS · Android

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

## Core features

| | |
| --- | --- |
| 🔒 **TLS 1.3 mutual authentication** | Every device holds its own Ed25519 key and self-signed certificate; all traffic is encrypted |
| 👤 **Explicit pairing + fingerprint check** | Discovery ≠ trust. You must compare certificate fingerprints on both devices and accept manually |
| 🖥 **Text and image sync** | Images travel as PNG binary chunks, **never base64** |
| 🔍 **Automatic LAN discovery** | mDNS (`_clipmesh._tcp.local.`), no configuration, no port forwarding |
| 📋 **Background clipboard watching** | The desktop side watches automatically; Android uses a foreground service plus a notification button |
| 🧩 **Platform-independent core** | `crates/**` depends on neither Tauri / Vue / Windows API / Android API |
| 🔁 **Loop prevention** | UUID dedup + echo suppression, so two devices never bounce the same item back at each other |

## Security model (present in the first version, not "later")

| Threat | Countermeasure |
| --- | --- |
| LAN eavesdropping | TLS 1.3, all traffic encrypted |
| Man-in-the-middle replacing a device | Certificate fingerprint pinning + the signature is bound to the TLS channel (see below) |
| Unpaired device reading the clipboard | An unpaired device **can only** send `PairRequest`; every other message is dropped |
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
├── crates/                    # completely platform-agnostic Rust core
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

Rust 1.85+ · Node 20+ · (Android additionally needs JDK 17+, the Android SDK and the NDK)

### Desktop

```bash
npm install                       # install frontend dependencies (npm workspaces)
npm run build                     # build the frontend first (required, see below)
cd apps/desktop
npm run dev                       # tauri dev: builds Rust and opens the window
```

> `tauri.conf.json`'s `frontendDist` points at `apps/desktop/ui/dist`, and
> `tauri::generate_context!()` reads that directory **at compile time** — when the
> directory does not exist, `cargo build` fails outright. So the frontend build has to
> run once before any Rust build.

Only want to look at the UI? **You do not need to compile Rust**:

```bash
npm --prefix apps/desktop/ui run dev     # http://localhost:1420
```

When no Tauri runtime is detected, the frontend switches to the built-in mock backend by itself,
showing three sample devices and history, with every interaction clickable. See
[`packages/ui-core/src/api/mock.ts`](packages/ui-core/src/api/mock.ts).

### Android

```bash
cd apps/android
npm run dev                       # tauri android dev
```

The full Android build steps (NDK variables, Gradle module wiring) are in [`docs/BUILD.md`](docs/BUILD.md).

### Tests

```bash
cargo test --workspace            # Rust: protocol / identity / security / engine / network / clipboard
npm run typecheck                 # frontend type check
npm run build                     # build the frontend for both apps
```

## First run

1. Start ClipMesh on both devices.
2. Within a few seconds they discover each other (the other one shows up in the device list).
3. On either one, click "Pair".
4. **The other one pops up a dialog showing the device name, platform and certificate fingerprint.**
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
| [`docs/BUILD.md`](docs/BUILD.md) | Build, packaging, Android wiring, troubleshooting |

## Protocol version

The current `PROTOCOL_VERSION = 1`. On a version mismatch the connection is rejected and
`ERROR_CODE_PROTOCOL_VERSION_MISMATCH` is reported back, instead of attempting a compatible parse.

## License

MIT, full text in [`LICENSE`](LICENSE).
