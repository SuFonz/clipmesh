# ClipMesh 架构设计

> 本文对应 `doc.txt` 的 **Phase 1：完整架构设计和模块职责**。
> 阅读顺序建议：§1 设计不变量 → §3 模块划分 → §4 安全模型 → §5 数据流。

---

## 1. 设计不变量

这五条是硬约束，任何实现细节都必须服从它们：

| # | 不变量 | 落地方式 |
| --- | --- | --- |
| 1 | **无中心服务器** | 没有 broker、没有中转、没有账号体系。设备通过 mDNS 互相发现，直连 TCP。 |
| 2 | **所有设备平等** | 没有 client/server 角色。TLS 双向认证，两端都既是监听方也是连接方，冲突时用 deviceId 字典序决定谁主动重连。 |
| 3 | **数据只在设备间传输** | 任何 payload 不经过第三方；凭据、证书、私钥永不离开本机。 |
| 4 | **第一版必须加密 + 信任** | TLS 1.3 双向认证 + Ed25519 设备身份 + 显式配对。没有"以后再加"的开关。 |
| 5 | **Rust Core 与平台无关** | `crates/**` 不依赖 Tauri / Vue / Win32 / Android API，可在任意平台编译和测试。 |

---

## 2. 总体架构

```
                    ┌──────────────────────────────┐
                    │   Vue 3 + TypeScript (UI)    │
                    │   桌面：侧边导航 / 多栏       │
                    │   移动：底部标签 / 单列       │
                    └───────────────┬──────────────┘
                                    │ invoke / event  (docs/IPC.md)
                    ┌───────────────┴──────────────┐
                    │        Tauri 2 Runtime       │
                    │  窗口 · 托盘 · 自启 · 命令层  │
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
│clipboard       │  │ mDNS + TCP + TLS   │  │ Ed25519 + 证书     │
│arboard / JNI   │  │                    │  │ + 信任库           │
└────────────────┘  └─────────┬──────────┘  └───────┬────────────┘
                            │                     │
                  ┌─────────┴──────────┐  ┌───────┴────────────┐
                  │ clipmesh-security  │  │ clipmesh-protocol  │
                  │ rustls 配置/校验    │  │ protobuf + 分帧     │
                  └────────────────────┘  └────────────────────┘
```

**依赖方向严格单向**（无环）：

```
protocol ← identity ← security ← core ← { clipboard, network } ← apps/*
```

`core` 只依赖 trait，**不依赖** `clipboard` / `network` 的具体实现。
具体的 arboard、mdns-sd、rustls 只在 `apps/*` 的组装代码里被 new 出来并注入。

这样做的直接收益：引擎可以用假 provider 做单元测试，且换一个平台只需要实现三个 trait。

---

## 3. 模块职责

### 3.1 `crates/protocol` — 线上契约

| 文件 | 职责 |
| --- | --- |
| `schema/clipmesh.proto` | protobuf3 定义。所有消息的**唯一**事实来源。 |
| `build.rs` | 用 `protox`（纯 Rust 编译器）编译 proto，**不需要 protoc 二进制**。 |
| `src/frame.rs` | 4 字节大端长度前缀分帧；读取前先校验长度上限，拒绝恶意超大帧。 |
| `src/message.rs` | `Envelope` 构造器与版本校验；`MessageKind` 分类。 |
| `src/clipboard.rs` | 强类型 payload 模型：`TextPayload` / `ImageMeta` / `ImagePayload`，包含大小上限与 sha256 校验。 |
| `src/device.rs` | `DeviceId`(UUIDv4) / `Platform` / `DeviceInfo`。 |

**为什么 payload 模型放在 protocol 而不是 core**：它是线上格式的 Rust 视图，
把它和 protobuf 放在一起，才能保证"改协议"和"改类型"永远发生在同一次提交里。

### 3.2 `crates/identity` — 设备身份

| 文件 | 职责 |
| --- | --- |
| `src/key.rs` | Ed25519 长期密钥对。私钥以 PKCS#8 存储，文件权限 0600。 |
| `src/certificate.rs` | 用 `rcgen` 生成**自签名 X.509**（CN = deviceId，SAN = `clipmesh.local`），导出 DER/PEM。 |
| `src/fingerprint.rs` | `sha256(cert DER)` → `A1B2 C3D4 …` 分组可读格式。 |
| `src/trust.rs` | 信任库：已配对设备的 JSON 持久化，原子写入。 |

设备身份在**首次启动**时生成并长期保存，包含 `deviceId` / `publicKey` / `privateKey`。
私钥只在本机，`publicKey` 用于身份验证。

### 3.3 `crates/security` — 传输安全

| 文件 | 职责 |
| --- | --- |
| `src/crypto.rs` | 签名/验签的领域封装：`sign_hello` / `verify_hello` / `sign_pair_accept`，域分隔前缀防重放。 |
| `src/tls.rs` | rustls 配置：两种策略 `TrustPolicy::Pairing`（接受未知设备，用于首次配对）与 `TrustPolicy::Strict`（只接受信任库内的指纹）。自定义 `ClientCertVerifier` / `ServerCertVerifier`。 |
| `src/session.rs` | 会话状态机 `TcpConnected → TlsEstablished → IdentityVerified → Active`，以及 TLS 通道绑定（exporter secret）。 |

### 3.4 `crates/core` — 同步引擎

| 文件 | 职责 |
| --- | --- |
| `src/provider.rs` | 三个 trait：`ClipboardProvider` / `NetworkProvider` / `IdentityProvider`。 |
| `src/sync.rs` | `DedupCache`（去重）、`EchoSuppressor`（回声抑制）、`SyncPolicy`、`History`。 |
| `src/device.rs` | `DeviceRegistry`：谁在线、谁在配对。**不存信任状态**，信任只存在于信任库。 |
| `src/manager.rs` | `SyncManager`：编排发现、连接、握手、收发、配对、事件广播。 |
| `src/event.rs` | 面向 UI 的快照事件与视图类型。 |

### 3.5 `crates/network` — 发现与传输

| 文件 | 职责 |
| --- | --- |
| `src/discovery.rs` | mDNS 广播与浏览（`_clipmesh._tcp.local.`），TXT 记录携带 `deviceId` / `name` / `platform` / `port` / `fp`。 |
| `src/tcp.rs` | 监听与连接，端口选择与冲突重试。 |
| `src/tls.rs` | 用 identity + security 组装 `TlsAcceptor` / `TlsConnector`。 |
| `src/connection.rs` | 单条会话的读写任务、握手、心跳、图片分片收发。 |
| `src/packet.rs` | 会话内的消息路由与 ACK。 |

### 3.6 `crates/clipboard` — 平台剪贴板

`ClipboardProvider` 的各平台实现：`windows.rs` / `linux.rs` / `macos.rs` / `android.rs`。
桌面统一走 `arboard`（文本 + 图片），Windows 额外用 `GetClipboardSequenceNumber` 做廉价变更检测。
Android 不直接调用系统 API，而是通过注入的 `AndroidClipboardHost` 把请求转给 Kotlin 插件——
这样 `clipmesh-clipboard` 依然与平台无关，可以被桌面编译。

---

## 4. 安全模型

### 4.1 威胁模型

| 威胁 | 对策 |
| --- | --- |
| 局域网内窃听 | TLS 1.3，全部流量加密。 |
| 中间人替换设备 | 证书指纹固定 + 应用层签名挑战绑定 TLS 通道（见 4.3）。 |
| 未授权设备读取剪贴板 | 默认只接受信任库内设备的会话；未配对设备仅能发 `PairRequest`。 |
| 设备身份伪造 | 身份 = Ed25519 公钥，`Hello` 必须用对应私钥签名，且签名绑定到当前 TLS 通道。 |
| 重放旧消息 | 每个 payload 带 UUID，接收方用 `DedupCache` 丢弃重复；握手挑战是一次性 32 字节随机数。 |
| 恶意超大帧耗尽内存 | 分帧层在分配前校验 16 MiB 上限（`MAX_FRAME_BYTES`）。 |
| 图片损坏/篡改 | `ClipboardImage.sha256`，写入剪贴板前校验。 |

### 4.2 配对流程

```
A 发现 B (mDNS)
  ↓
A 建立 TCP + TLS（此时用 Pairing 策略：接受未知证书，但记录指纹）
  ↓
A → PairRequest { deviceId, name, platform, publicKey, certificate, fingerprint, nonce }
  ↓
B 弹出确认框：设备名 / 平台 / 证书指纹 + [接受] [拒绝]
  ↓
B 接受 → PairAccept { ..., signature = sign(nonce ‖ B.deviceId) }
  ↓
双方写入信任库：
  TrustedDevice { deviceId, name, publicKey, certificate, fingerprint, trustedAt }
```

指纹的**带外比对**（两块屏幕对照）才是真正的安全边界；
签名只是保证"接受"这个动作确实来自持有该私钥的设备，而不是被局域网里的第三方伪造。

### 4.3 通道绑定（防中继）

握手时双方各自生成 32 字节 `challenge`，签名内容为：

```
sha256( "clipmesh-hello-v1" ‖ challenge ‖ channel_binding )
```

其中 `channel_binding = TLS exporter secret`（`export_keying_material(b"EXPORTER-clipmesh-identity")`）。
exporter secret 只有这条 TLS 连接的两端能算出，因此攻击者无法把 A 的 `Hello`
原样转发到另一条连接上冒充 A —— 这挡住了"TCP 层中继"这一整类攻击。

### 4.4 连接状态机

```
TCP Connect
   ↓
TLS Handshake（双向认证，两侧都出示自签证书）
   ↓
Certificate 验证（自签名合法性 + 指纹策略）
   ↓
Device Identity 验证（Hello 签名 + 通道绑定）
   ↓
建立 Session（写入连接表，开始投递 payload）
   ↓
传输数据
```

任何一步失败 → 发送 `ErrorMessage` 并关闭连接，绝不降级为明文。

---

## 5. 数据流

### 5.1 本地复制 → 远端（发送路径）

```
系统剪贴板变化
  ↓ ClipboardProvider::watch()
EchoSuppressor 判断是不是我们自己刚写进去的 → 是则丢弃
  ↓
SyncPolicy 允许？（auto_sync / sync_text / sync_images / 大小）
  ↓
构造 TextPayload(id = UUIDv4) 或 ImageMeta(sha256)
  ↓ 记入 DedupCache（自己的 id 也记，防止对端回传）
NetworkProvider::broadcast(Envelope)
  ↓
每个在线且已信任的会话：TLS 写入分帧
  ↓ 图片：ClipboardImage 元数据帧 + N × ImageChunk(64 KiB) 二进制分片
UI 事件 clipmesh://clipboard-sent { item, delivered }
```

### 5.2 远端 → 本地（接收路径）

```
TLS 读到 Envelope
  ↓
Envelope::ensure_valid()（协议版本 + payload 存在）
  ↓
会话已通过 Identity 验证？否则只允许 PairRequest
  ↓
DedupCache::insert(id) → 已见过则直接丢弃（这就是防环的关键）
  ↓
图片：收齐所有分片 → 拼接 → meta.verify(&data) 校验 sha256
  ↓
EchoSuppressor::record_write(&content)
ClipboardProvider::write(&content)
  ↓
UI 事件 clipmesh://clipboard-received { item }
```

### 5.3 防环说明

两个都开着自动同步的设备会互相回弹同一条内容。三重防护：

1. **id 去重**：Origin 生成 UUID，任何设备处理过一次就不再处理（`DedupCache`）。
2. **回声抑制**：写入本机剪贴板前记录内容签名（kind + len + sha256），
   10 秒内匹配到同签名的变化视为自己造成的回声（`EchoSuppressor`）。
3. **不回传 origin**：`source_device` 等于本机的 payload 直接丢弃。

### 5.4 为什么 v1 不做中继转发

payload 直接广播给**每一个已连接且已信任**的设备，不做存储转发、不做多跳中继。

理由：局域网内每台设备都通过 mDNS 发现其他所有设备，A→C 的直连一定存在，
中继只会在"某些设备之间连不上"时才有意义（那是跨网段场景，v1 不在范围内）；
而一旦引入中继，"这条 payload 是谁转发的、能不能信"就变成一个需要重新论证的问题。

代价是三台设备时会产生 A→B、A→C 两条连接而不是一条链，
在局域网上这比中继更快也更简单。`DedupCache` 依然必需 ——
对端把我们的 payload 回弹回来是真实存在的失败模式。

### 5.5 谁主动连接

两端都会发现对方，若同时发起就会出现两条会话。规则：
**deviceId 字典序较小的一方主动拨号**，另一方只监听。
`SyncManager` 在收到 `Discovered` 事件时按这条规则决定是否调用 `connect()`。

---

## 6. 平台实现

### 6.1 Desktop（`apps/desktop`）

| 层 | 职责 |
| --- | --- |
| Rust | 剪贴板监听、TCP/TLS、mDNS、身份验证、后台运行 |
| Tauri | 窗口、系统托盘、设置、开机自启 |
| Vue | `DesktopLayout`：侧边导航 + 多栏 + 状态栏 |

托盘菜单：显示主窗口 / 立即广播剪贴板 / 暂停自动同步 / 退出。

### 6.2 Android（`apps/android`）

Android 的后台限制要求把常驻能力放进原生插件：

```
Vue → Tauri → Rust → Android Native Plugin → Android API
```

插件三块（`apps/android/plugins/bridge/`）：

| 目录 | 职责 |
| --- | --- |
| `foregroundservice/` | 常驻前台服务，维持进程与网络会话；通知栏常驻，带「广播剪贴板」按钮。 |
| `notification/` | 通知的构造与投递：常驻服务通知与「收到剪贴板」通知。 |
| `broadcast/` | 透明 Activity 与进程级 handoff：通知按钮把剪贴板读出来交给 Rust，界面不出现；读不到时回退到可见路径。 |
| `clipboard/` | `ClipboardManager` 读写；Android 10+ 后台读剪贴板受限，因此广播由用户点击通知按钮**主动触发**。 |

Android 上 Rust 不直接调 `ClipboardManager`，而是通过 `AndroidClipboardHost` trait
把请求交给 Kotlin 实现，保持 `crates/**` 的平台无关性。

---

## 7. 目录结构

```
clipmesh/
├── Cargo.toml                 # Rust workspace
├── package.json               # npm workspaces 根
├── crates/
│   ├── protocol/              # protobuf + 分帧 + payload 模型
│   ├── identity/              # Ed25519 / 证书 / 指纹 / 信任库
│   ├── security/              # rustls 配置 / 校验器 / 会话
│   ├── core/                  # 引擎：trait、去重、设备表、编排
│   ├── network/               # mDNS + TCP + TLS
│   └── clipboard/             # 各平台剪贴板实现
├── packages/ui-core/          # 共享前端：类型 / store / API / 通用组件
├── apps/
│   ├── desktop/{src-tauri,ui} # 桌面 Tauri 宿主 + 桌面布局
│   └── android/
│       ├── src-tauri/         # 移动 Tauri 宿主
│       ├── ui/                # 移动布局
│       └── plugins/bridge/{clipboard,notification,foregroundservice}
└── docs/
```

> 与 `doc.txt` 的两点差异，均为 Tauri 2 的实际约束：
> 1. `packages/ui-core/` 是为了让两端"共享数据模型/状态管理/API/通用组件"而不用复制代码；
>    两个 app 各自保留 `layouts/` 与 `views/`，因为两套布局本来就不一样。
> 2. Android 端 `gen/android` 由 `tauri android init` 生成并纳入版本库，
>    原生插件以独立 Gradle module 形式被 `settings.gradle` 引入。

---

## 8. 阶段对照

| 阶段 | 内容 | 产物 |
| --- | --- | --- |
| Phase 1 | 架构设计 | 本文档 |
| Phase 2 | protobuf 协议 | `crates/protocol`（27 个单元测试） |
| Phase 3 | identity + TLS | `crates/identity`、`crates/security` |
| Phase 4 | Rust Core | `crates/core`、`crates/clipboard` |
| Phase 5 | mDNS + TCP + TLS | `crates/network` |
| Phase 6 | 桌面端 | `apps/desktop` |
| Phase 7 | Android + Native Plugin | `apps/android` |
| Phase 8 | 测试/优化/打包 | `docs/BUILD.md`、CI |
