# ClipMesh frontend ↔ Rust IPC contract

**English** | [简体中文](IPC.zh-CN.md)

> This file is a **frozen contract**. The Vue side and the Tauri side must implement exactly this; a change on either side has to update this file at the same time.
> The Rust-side definitions live in `crates/core/src/event.rs` (view types) and `apps/desktop/src-tauri/src/commands.rs` (commands).

## 1. Conventions

| Item | Rule |
| --- | --- |
| Command names | `snake_case`, called through `invoke("name", { args })` |
| Argument names | `camelCase` (Tauri 2 maps JS camelCase onto Rust snake_case by default) |
| Returned structures | `camelCase` fields (Rust side `#[serde(rename_all = "camelCase")]`) |
| Timestamps | `number`, Unix milliseconds |
| Device ID | `string`, UUIDv4 |
| Errors | `invoke` throws a string (the `Display` of the Rust `CoreError`) |
| Event names | `clipmesh://<name>`, payload is the TS type below |

**Events are snapshots, not deltas**: when `clipmesh://peers` arrives, replace the local list with the whole array; no merge logic is needed.
The frontend calls the matching `list_*` command once at startup for the initial fill, and after that relies on events alone.

---

## 2. TypeScript data types

```ts
export type Platform = "windows" | "linux" | "macos" | "android" | "unknown";
export type ClipboardKind = "text" | "image";
export type PairingDirection = "incoming" | "outgoing";

/** This machine's status. */
export interface StatusView {
  running: boolean;          // whether the engine (discovery + listener) is running
  autoSync: boolean;         // whether local clipboard changes are pushed automatically
  deviceId: string;
  deviceName: string;
  platform: Platform;
  fingerprint: string;       // shaped like "A1B2 C3D4 E5F6 ..."
  listenPort: number;
  connectedPeers: number;
  trustedPeers: number;
  lastError: string | null;
}

/** A device currently visible on the network. */
export interface PeerView {
  deviceId: string;
  name: string;
  platform: Platform;
  address: string;           // "192.168.1.7:47711"
  fingerprint: string;
  trusted: boolean;          // whether it is already in the trust list
  connected: boolean;        // whether a TLS session has been established
  pairing: boolean;          // whether pairing is in progress
  lastSeen: number;          // Unix milliseconds
}

/** A trusted device. */
export interface TrustedDeviceView {
  deviceId: string;
  name: string;
  platform: Platform;
  fingerprint: string;
  trustedAt: number;
  online: boolean;
}

/** A pairing request waiting for the user's decision. */
export interface PairingPrompt {
  deviceId: string;
  name: string;
  platform: Platform;
  fingerprint: string;
  address: string;
  requestedAt: number;
  direction: PairingDirection;
}

/** A history entry (image entries carry metadata only, no pixels). */
export type ClipboardItemView =
  | {
      kind: "text";
      id: string;
      sourceDevice: string;
      timestamp: number;
      content: string;
    }
  | {
      kind: "image";
      id: string;
      sourceDevice: string;
      timestamp: number;
      mime: string;      // "image/png"
      size: number;      // bytes
      width: number;
      height: number;
    };

/** Interface language. `system` follows the OS, the rest are explicit overrides. */
export type LanguageSetting = "system" | "zh-CN" | "en";

/** User settings. */
export interface SettingsView {
  deviceName: string;
  autoSync: boolean;          // listen in the background and sync automatically
  syncText: boolean;
  syncImages: boolean;
  maxImageBytes: number;
  startMinimized: boolean;    // desktop: start minimized to the tray
  launchAtLogin: boolean;     // desktop: launch at login
  androidForegroundService: boolean; // Android: keep the foreground service running
  language: LanguageSetting;  // unknown values degrade to "system" on the Rust side
}

/** The result of a send. */
export interface SendResult {
  id: string;
  delivered: number;          // how many online devices it was delivered to
}

/** This machine's identity. */
export interface IdentityView {
  deviceId: string;
  deviceName: string;
  platform: Platform;
  fingerprint: string;
  publicKey: string;          // hex
  certificatePem: string;     // for the user to compare or export
}
```

---

## 3. Commands (`invoke`)

### State and lists

| Command | Arguments | Returns |
| --- | --- | --- |
| `get_status` | — | `StatusView` |
| `list_peers` | — | `PeerView[]` |
| `list_trusted_devices` | — | `TrustedDeviceView[]` |
| `list_pairing_requests` | — | `PairingPrompt[]` |
| `get_history` | — | `ClipboardItemView[]` |
| `get_settings` | — | `SettingsView` |
| `get_identity` | — | `IdentityView` |

### Lifecycle

| Command | Arguments | Returns | Notes |
| --- | --- | --- | --- |
| `start_engine` | — | `StatusView` | Idempotent |
| `stop_engine` | — | `StatusView` | Idempotent |
| `update_settings` | `{ patch: Partial<SettingsView> }` | `SettingsView` | Takes effect immediately and is persisted |
| `set_device_name` | `{ name: string }` | `IdentityView` | Re-announces over mDNS |

### Pairing and trust

| Command | Arguments | Returns | Notes |
| --- | --- | --- | --- |
| `request_pairing` | `{ deviceId: string }` | `void` | Start pairing with a discovered device |
| `respond_pairing` | `{ deviceId: string, accept: boolean }` | `void` | Answer a pairing request that came in |
| `unpair_device` | `{ deviceId: string }` | `void` | Remove from the trust list and disconnect |

### Clipboard

| Command | Arguments | Returns | Notes |
| --- | --- | --- | --- |
| `send_clipboard` | — | `SendResult` | **Read the current system clipboard and broadcast it** (the Android notification's "broadcast clipboard" button goes through this one) |
| `send_text` | `{ content: string }` | `SendResult` | Broadcast the given text directly |
| `resend_history_item` | `{ id: string }` | `SendResult` | Send a history entry again |
| `copy_history_item` | `{ id: string }` | `void` | Write to the local clipboard only, without sending |
| `clear_history` | — | `void` | |
| `get_image_thumbnail` | `{ id: string, maxSize: number }` | `string` | Returns a **data URL**, already downscaled. The pixels come from this machine's `<state>/images/<id>.png` copy and have nothing to do with what is currently on the clipboard |

> `get_image_thumbnail` is the only place in this project that uses base64, and it only serves local UI thumbnails.
> Images on the wire are always raw PNG binary chunks (see `crates/protocol/src/frame.rs`).

### Android-only

| Command | Arguments | Returns | Notes |
| --- | --- | --- | --- |
| `android_start_service` | — | `void` | Start the foreground service |
| `android_stop_service` | — | `void` | Stop the foreground service |
| `android_service_running` | — | `boolean` | |
| `android_notification_permission` | — | `boolean` | Only **queries** the Android 13+ notification permission, shows no dialog |
| `android_request_notification_permission` | — | `boolean` | Android 13+ notification permission, shows the system dialog |

Calling these commands on a non-Android platform throws `"android commands are only available on Android"`.

---

## 4. Events (`listen`)

The frontend only has to mount `useCoreEvents()` once in `App.vue`; it writes every event into the Pinia store.

| Event name | Payload | When it fires |
| --- | --- | --- |
| `clipmesh://status` | `StatusView` | Run state, online count or error changed |
| `clipmesh://peers` | `PeerView[]` | A device appeared, disappeared or changed connection state |
| `clipmesh://trusted` | `TrustedDeviceView[]` | The trust list changed |
| `clipmesh://pairing-requests` | `PairingPrompt[]` | Someone asked to pair, or a request was handled |
| `clipmesh://history` | `ClipboardItemView[]` | The history changed (at most 50 entries, newest first) |
| `clipmesh://clipboard-received` | `ClipboardItemView` | A remote clipboard item arrived and was written locally |
| `clipmesh://clipboard-sent` | `{ item: ClipboardItemView, delivered: number }` | Local content was pushed |
| `clipmesh://error` | `string` | A non-fatal error, for a toast |

---

## 5. Platforms and layout

The two apps **share**: types, the Pinia store, the `invoke` wrapper, common components, `useCoreEvents`.

The two apps do **not** share a layout; each one implements its own:

| | Desktop (`apps/desktop/ui`) | Android (`apps/android/ui`) |
| --- | --- | --- |
| Layout | Side navigation + multiple columns | Bottom tab bar + single column |
| Interaction | Hover, right-click, keyboard shortcuts | Large buttons, touch targets ≥48px |
| Views | Dashboard / Devices / History / Settings / Pairing | Home / Devices / History / Settings |
| Special | System tray, window controls | Foreground-service toggle, broadcast button, notification permission |

How the choice is made: `useDeviceType()` returns `"desktop" | "mobile"` from `platform` and the window width,
but **each app statically mounts its own Layout**; there is no runtime either/or, so both layouts never end up in one bundle.
