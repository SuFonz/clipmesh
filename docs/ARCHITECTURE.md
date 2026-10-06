# ClipMesh Architecture

**English** | [简体中文](ARCHITECTURE.zh-CN.md)

> This document covers **Phase 1: complete architecture design and module responsibilities** from `doc.txt`.
> Suggested reading order: §1 Design invariants → §3 Module breakdown → §4 Security model → §5 Data flow.

---

## 1. Design invariants

These five are hard constraints; every implementation detail must obey them:

| # | Invariant | How it is enforced |
| --- | --- | --- |
| 1 | **No central server** | No broker, no relay, no account system. Devices discover each other over mDNS and connect over direct TCP. |
| 2 | **All devices are equal** | There is no client/server role. TLS mutual authentication; both ends listen and both ends connect, and on collision deviceId lexicographic order decides who redials. |
| 3 | **Data only travels between devices** | No payload passes through a third party; credentials, certificates and private keys never leave the local machine. |
| 4 | **v1 must be encrypted + trusted** | TLS 1.3 mutual authentication + Ed25519 device identity + explicit pairing. There is no "add it later" switch. |
| 5 | **The Rust Core is platform-independent** | `crates/**` does not depend on Tauri / Vue / Win32 / Android APIs, and compiles and tests on any platform. |

---

## 2. Overall architecture

```
                    ┌──────────────────────────────┐
                    │   Vue 3 + TypeScript (UI)    │
                    │  Desktop: side nav / columns │
                    │  Mobile: bottom tabs / list  │
                    └───────────────┬──────────────┘
                                    │ invoke / event  (docs/IPC.md)
                    ┌───────────────┴──────────────┐
                    │        Tauri 2 Runtime       │
                    │   window · tray · autostart  │
                    │   command layer              │
                    └───────────────┬──────────────┘
                                    │ Arc<SyncManager>
┌───────────────────────────────────┴───────────────────────────────────┐
│                          clipmesh-core                                │
│   SyncManager ── DeviceRegistry ── DedupCache / EchoSuppressor        │
│         │                                                             │
│         ├── ClipboardProvider (trait)                                 │
│         ├── NetworkProvider   (trait)                                 │
│         └── IdentityProvider  (trait)                                 │
└───────┬─────────────────────┬─────────────────────┬───────────────────┘
        │                     │                     │
┌───────┴────────┐  ┌─────────┴──────────┐  ┌───────┴────────────┐
│clipmesh-       │  │ clipmesh-network   │  │ clipmesh-identity  │
│clipboard       │  │ mDNS + TCP + TLS   │  │ Ed25519 + certs    │
│arboard / JNI   │  │                    │  │ + trust store      │
└────────────────┘  └─────────┬──────────┘  └───────┬────────────┘
                              │                     │
                    ┌─────────┴──────────┐  ┌───────┴────────────┐
                    │ clipmesh-security  │  │ clipmesh-protocol  │
                    │ rustls config /    │  │ protobuf + framing │
                    │ verification       │  │                    │
                    └────────────────────┘  └────────────────────┘
```

**The dependency direction is strictly one-way** (acyclic):

```
protocol ← identity ← security ← core ← { clipboard, network } ← apps/*
```

`core` depends only on traits and **not** on the concrete `clipboard` / `network` implementations.
The concrete arboard, mdns-sd and rustls objects are only created and injected in the wiring code under `apps/*`.

The immediate payoff: the engine can be unit-tested with fake providers, and supporting another platform only means implementing three traits.

---

## 3. Module responsibilities

### 3.1 `crates/protocol` — the wire contract

| File | Responsibility |
| --- | --- |
| `schema/clipmesh.proto` | protobuf3 definition. The **single** source of truth for all messages. |
| `build.rs` | Compiles the proto with `protox` (a pure-Rust compiler); **no protoc binary required**. |
| `src/frame.rs` | 4-byte big-endian length-prefix framing; the length cap is checked before reading, rejecting malicious oversized frames. |
| `src/message.rs` | `Envelope` constructor and version checks; `MessageKind` classification. |
| `src/clipboard.rs` | Strongly-typed payload models: `TextPayload` / `ImageMeta` / `ImagePayload`, including size caps and sha256 verification. |
| `src/device.rs` | `DeviceId`(UUIDv4) / `Platform` / `DeviceInfo`. |

**Why the payload models live in protocol rather than core**: they are the Rust view of the wire format,
and keeping them next to the protobuf is the only way to guarantee that "change the protocol" and "change the types" always happen in the same commit.

### 3.2 `crates/identity` — device identity

| File | Responsibility |
| --- | --- |
| `src/key.rs` | Ed25519 long-term key pair. The private key is stored as PKCS#8 with file permissions 0600. |
| `src/certificate.rs` | Uses `rcgen` to generate a **self-signed X.509** (CN = deviceId, SAN = `clipmesh.local`), exported as DER/PEM. |
| `src/fingerprint.rs` | `sha256(cert DER)` → `A1B2 C3D4 …` grouped, human-readable format. |
| `src/trust.rs` | Trust store: JSON persistence of paired devices, atomic writes. |

Device identity is generated on **first launch** and kept long-term; it contains `deviceId` / `publicKey` / `privateKey`.
The private key stays on the local machine only; `publicKey` is used for authentication.

### 3.3 `crates/security` — transport security

| File | Responsibility |
| --- | --- |
| `src/crypto.rs` | Domain wrapper around signing/verification: `sign_hello` / `verify_hello` / `sign_pair_accept`, with domain-separation prefixes to prevent replay. |
| `src/tls.rs` | rustls configuration: two policies, `TrustPolicy::Pairing` (accepts unknown devices, used for first-time pairing) and `TrustPolicy::Strict` (accepts only fingerprints from the trust store). Custom `ClientCertVerifier` / `ServerCertVerifier`. |
| `src/session.rs` | Session state machine `TcpConnected → TlsEstablished → IdentityVerified → Active`, plus TLS channel binding (exporter secret). |

### 3.4 `crates/core` — sync engine

| File | Responsibility |
| --- | --- |
| `src/provider.rs` | The three traits: `ClipboardProvider` / `NetworkProvider` / `IdentityProvider`. |
| `src/sync.rs` | `DedupCache` (deduplication), `EchoSuppressor` (echo suppression), `SyncPolicy`, `History`. |
| `src/device.rs` | `DeviceRegistry`: who is online, who is pairing. It does **not store trust state**; trust exists only in the trust store. |
| `src/manager.rs` | `SyncManager`: orchestrates discovery, connection, handshake, send/receive, pairing and event broadcast. |
| `src/event.rs` | UI-facing snapshot events and view types. |

### 3.5 `crates/network` — discovery and transport

| File | Responsibility |
| --- | --- |
| `src/discovery.rs` | mDNS advertise and browse (`_clipmesh._tcp.local.`); TXT records carry `deviceId` / `name` / `platform` / `port` / `fp`. |
| `src/tcp.rs` | Listening and connecting, port selection and conflict retry. |
| `src/tls.rs` | Assembles `TlsAcceptor` / `TlsConnector` from identity + security. |
| `src/connection.rs` | Read/write tasks for a single session, handshake, heartbeat, image chunk send/receive. |
| `src/packet.rs` | Message routing and ACK within a session. |

### 3.6 `crates/clipboard` — platform clipboard

The per-platform implementations of `ClipboardProvider`: `windows.rs` / `linux.rs` / `macos.rs` / `android.rs`.
Desktop goes through `arboard` everywhere (text + images); Windows additionally uses `GetClipboardSequenceNumber` for cheap change detection.
Android does not call the system API directly — an injected `AndroidClipboardHost` forwards requests to the Kotlin plugin,
so `clipmesh-clipboard` stays platform-independent and can still be compiled on desktop.

---

## 4. Security model

### 4.1 Threat model

| Threat | Countermeasure |
| --- | --- |
| Eavesdropping on the LAN | TLS 1.3, all traffic encrypted. |
| Man-in-the-middle replacing a device | Certificate fingerprint pinning + an application-layer signature challenge bound to the TLS channel (see 4.3). |
| Unauthorized device reading the clipboard | By default only sessions from devices in the trust store are accepted; an unpaired device can only send `PairRequest`. |
| Device identity spoofing | Identity = the Ed25519 public key, `Hello` must be signed with the matching private key, and the signature is bound to the current TLS channel. |
| Replaying old messages | Every payload carries a UUID and the receiver drops duplicates with `DedupCache`; the handshake challenge is a one-time 32-byte random value. |
| Malicious oversized frame exhausting memory | The framing layer checks the 16 MiB cap (`MAX_FRAME_BYTES`) before allocating. |
| Corrupted or tampered images | `ClipboardImage.sha256`, verified before writing to the clipboard. |

### 4.2 Pairing flow

```
A discovers B (mDNS)
  ↓
A establishes TCP + TLS (the Pairing policy is used here: unknown certificates are accepted, but the fingerprint is recorded)
  ↓
A → PairRequest { deviceId, name, platform, publicKey, certificate, fingerprint, nonce }
  ↓
B pops up a confirmation dialog: device name / platform / certificate fingerprint + [Accept] [Reject]
  ↓
B accepts → PairAccept { ..., signature = sign(nonce ‖ B.deviceId) }
  ↓
Both sides write to the trust store:
  TrustedDevice { deviceId, name, publicKey, certificate, fingerprint, trustedAt }
```

The **out-of-band comparison** of the fingerprints (two screens side by side) is the real security boundary;
the signature only guarantees that the act of "accepting" really came from the device holding that private key,
rather than being forged by a third party on the LAN.

### 4.3 Channel binding (anti-relay)

During the handshake each side generates a 32-byte `challenge`, and the signed content is:

```
sha256( "clipmesh-hello-v1" ‖ challenge ‖ channel_binding )
```

where `channel_binding = TLS exporter secret` (`export_keying_material(b"EXPORTER-clipmesh-identity")`).
Only the two ends of this TLS connection can compute the exporter secret, so an attacker cannot forward A's `Hello`
verbatim onto another connection and impersonate A there — this blocks the whole class of "TCP-layer relay" attacks.

### 4.4 Connection state machine

```
TCP Connect
   ↓
TLS Handshake (mutual authentication, both sides present a self-signed certificate)
   ↓
Certificate verification (self-signed validity + fingerprint policy)
   ↓
Device Identity verification (Hello signature + channel binding)
   ↓
Establish the Session (write into the connection table, start delivering payloads)
   ↓
Transfer data
```

Failure at any step → send an `ErrorMessage` and close the connection; never downgrade to plaintext.

---

## 5. Data flow

### 5.1 Local copy → remote (send path)

```
System clipboard changes
  ↓ ClipboardProvider::watch()
EchoSuppressor decides whether this is something we just wrote ourselves → if so, drop it
  ↓
Does SyncPolicy allow it? (auto_sync / sync_text / sync_images / size)
  ↓
Build TextPayload(id = UUIDv4) or ImageMeta(sha256)
  ↓ record it in DedupCache (our own id as well, so the peer cannot bounce it back)
NetworkProvider::broadcast(Envelope)
  ↓
Every online and trusted session: TLS framed write
  ↓ images: ClipboardImage metadata frame + N × ImageChunk(64 KiB) binary chunks
UI event clipmesh://clipboard-sent { item, delivered }
```

### 5.2 Remote → local (receive path)

```
TLS reads an Envelope
  ↓
Envelope::ensure_valid() (protocol version + payload present)
  ↓
Has the session passed Identity verification? If not, only PairRequest is allowed
  ↓
DedupCache::insert(id) → already seen, drop it (this is the key to loop prevention)
  ↓
Images: collect every chunk → reassemble → meta.verify(&data) checks the sha256
  ↓
EchoSuppressor::record_write(&content)
ClipboardProvider::write(&content)
  ↓
UI event clipmesh://clipboard-received { item }
```

### 5.3 Loop prevention

Two devices that both have auto-sync on will bounce the same item back at each other. Three layers of protection:

1. **id dedup**: the origin generates a UUID, and no device processes it a second time (`DedupCache`).
2. **Echo suppression**: before writing to the local clipboard, the content signature (kind + len + sha256) is recorded;
   a change matching that signature within 10 seconds counts as an echo we caused ourselves (`EchoSuppressor`).
3. **Never send back to the origin**: a payload whose `source_device` is the local device is dropped immediately.

### 5.4 Why v1 does not relay

A payload is broadcast directly to **every connected and trusted** device: no store-and-forward, no multi-hop relaying.

Rationale: on a LAN every device discovers every other device over mDNS, so a direct A→C link always exists;
relaying would only be meaningful when "some devices cannot reach each other" (that is the cross-subnet case, out of scope for v1);
and once relaying is introduced, "who forwarded this payload, and can it be trusted" becomes a question that has to be argued from scratch.

The cost is that with three devices you get two links, A→B and A→C, instead of one chain —
on a LAN that is faster and simpler than relaying. `DedupCache` is still required:
a peer bouncing our payload back is a real failure mode.

### 5.5 Who dials

Both ends discover each other, and dialling at the same time would produce two sessions. The rule:
**the side with the smaller deviceId dials**, the other side only listens.
When `SyncManager` receives a `Discovered` event it applies this rule to decide whether to call `connect()`.

---

## 6. Platform implementations

### 6.1 Desktop (`apps/desktop`)

| Layer | Responsibility |
| --- | --- |
| Rust | Clipboard watching, TCP/TLS, mDNS, identity verification, background operation |
| Tauri | Window, system tray, settings, launch at login |
| Vue | `DesktopLayout`: side navigation + multiple columns + status bar |

Tray menu: show main window / broadcast clipboard now / pause auto-sync / quit.

### 6.2 Android (`apps/android`)

Android's background restrictions require the resident capabilities to live in a native plugin:

```
Vue → Tauri → Rust → Android Native Plugin → Android API
```

The plugin has three parts (`apps/android/plugins/bridge/`):

| Directory | Responsibility |
| --- | --- |
| `foregroundservice/` | Resident foreground service that keeps the process and the network sessions alive; a persistent notification with a "broadcast clipboard" button. |
| `notification/` | Building and posting notifications: the foreground-service notification and the "clipboard received" notification. |
| `broadcast/` | Transparent Activity and process-level handoff: the notification button reads the clipboard and hands it to Rust without any UI appearing; falls back to a visible path when the read fails. |
| `clipboard/` | `ClipboardManager` reads and writes; on Android 10+ background clipboard reads are restricted, so a broadcast is **triggered explicitly** by the user tapping the notification button. |

On Android, Rust does not call `ClipboardManager` directly; it hands requests to the Kotlin implementation through the
`AndroidClipboardHost` trait, which keeps `crates/**` platform-independent.

### 6.3 UI language (`packages/ui-core/src/i18n/`)

The UI supports two languages, `zh-CN` / `en`. **No i18n library is pulled in**: the whole UI needs only "two tables + one lookup function",
and hand-writing one is smaller and more controllable than dragging in vue-i18n.

```
settings.json ──language──▶ Settings (Rust) ──get_settings──▶ settings store
                                                                  │ applyLanguageSetting()
                                                                  ▼
                                       i18n module's locale ref ──▶ every t() call site
```

Four rules:

1. **Persistence goes through the existing settings**, not `localStorage`. `Settings.language` only ever takes
   `system` / `zh-CN` / `en`; on the Rust side `Language::from_tag` narrows any unrecognised tag to
   `system` (the same idea as clamping `history_capacity`: never introduce a fourth value, and never let the file brick the app).
2. **`system` is resolved in the frontend**: only the first entry of `navigator.language` is looked at; anything starting
   with `zh` counts as Chinese, everything else as English. The later entries of `navigator.languages` are deliberately
   ignored — that is a preference list, and it does not mean the UI should use that language.
3. **Message tables are split by domain** (`messages/{common,settings,components,desktop,android}.ts`),
   `catalog.ts` assembles them into one table, `MessageKey = keyof typeof catalog`.
   Every message gives both languages in the same object literal, so **omitting one is a compile error**;
   `t()` only accepts `MessageKey`, so a mistyped key is likewise a compile error rather than a blank at runtime.
4. **Switching is live**: `locale` is a `computed`, `t()` reads it inside templates / `computed`s,
   and the affected fragments re-render automatically when the language changes — no refresh needed.

Convention: **any new user-visible text must go into the message tables**; do not leave literals in components.
Code comments stay in Chinese (repository convention), and developer diagnostics such as `console.*` / `tracing` are not translated.
Route titles store a **message key** (`meta.title`) rather than a finished sentence — when vue-router merges the `meta`
of nested routes it flattens the values, so a stored string would stay frozen at module-load time forever.

---

## 7. Directory structure

```
clipmesh/
├── Cargo.toml                 # Rust workspace
├── package.json               # npm workspaces root
├── crates/
│   ├── protocol/              # protobuf + framing + payload models
│   ├── identity/              # Ed25519 / certificates / fingerprints / trust store
│   ├── security/              # rustls config / verifiers / session
│   ├── core/                  # engine: traits, dedup, device table, orchestration
│   ├── network/               # mDNS + TCP + TLS
│   └── clipboard/             # per-platform clipboard implementations
├── packages/ui-core/          # shared frontend: types / store / API / i18n / common components
├── apps/
│   ├── desktop/{src-tauri,ui} # desktop Tauri host + desktop layout
│   └── android/
│       ├── src-tauri/         # mobile Tauri host
│       ├── ui/                # mobile layout
│       └── plugins/bridge/{clipboard,notification,foregroundservice}
└── docs/
```

> Two differences from `doc.txt`, both forced by Tauri 2's actual constraints:
> 1. `packages/ui-core/` exists so that both ends can "share the data model / state management / API / common components" without copying code;
>    the two apps each keep their own `layouts/` and `views/`, because the two layouts were never the same to begin with.
> 2. On Android, `gen/android` is generated by `tauri android init` and checked into version control,
>    and the native plugin is pulled in by `settings.gradle` as a standalone Gradle module.

---

## 8. Phase map

| Phase | Content | Deliverable |
| --- | --- | --- |
| Phase 1 | Architecture design | This document |
| Phase 2 | protobuf protocol | `crates/protocol` (27 unit tests) |
| Phase 3 | identity + TLS | `crates/identity`, `crates/security` |
| Phase 4 | Rust Core | `crates/core`, `crates/clipboard` |
| Phase 5 | mDNS + TCP + TLS | `crates/network` |
| Phase 6 | Desktop | `apps/desktop` |
| Phase 7 | Android + Native Plugin | `apps/android` |
| Phase 8 | Testing / optimization / packaging | `docs/BUILD.md`, CI |
