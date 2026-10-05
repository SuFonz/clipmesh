# ClipMesh 前端 ↔ Rust IPC 契约

> 本文件是 **冻结契约**。Vue 端与 Tauri 端必须严格按此实现，任何一方改动都要同步更新本文件。
> Rust 侧定义见 `crates/core/src/event.rs`（视图类型）与 `apps/desktop/src-tauri/src/commands.rs`（命令）。

## 1. 约定

| 项目 | 规则 |
| --- | --- |
| 命令名 | `snake_case`，通过 `invoke("命令名", { 参数 })` 调用 |
| 参数名 | `camelCase`（Tauri 2 默认把 JS 的 camelCase 映射到 Rust 的 snake_case） |
| 返回结构 | `camelCase` 字段（Rust 侧 `#[serde(rename_all = "camelCase")]`） |
| 时间戳 | `number`，Unix 毫秒 |
| 设备 ID | `string`，UUIDv4 |
| 错误 | `invoke` 抛出字符串（Rust `CoreError` 的 `Display`） |
| 事件名 | `clipmesh://<name>`，负载为下方 TS 类型 |

**事件是快照而不是增量**：收到 `clipmesh://peers` 就用整个数组替换本地列表，不需要合并逻辑。
前端在启动时调用一次对应的 `list_*` 命令做初始填充，之后只靠事件更新。

---

## 2. TypeScript 数据类型

```ts
export type Platform = "windows" | "linux" | "macos" | "android" | "unknown";
export type ClipboardKind = "text" | "image";
export type PairingDirection = "incoming" | "outgoing";

/** 本机状态。 */
export interface StatusView {
  running: boolean;          // 引擎（发现 + 监听）是否在运行
  autoSync: boolean;         // 本地剪贴板变化是否自动推送
  deviceId: string;
  deviceName: string;
  platform: Platform;
  fingerprint: string;       // 形如 "A1B2 C3D4 E5F6 ..."
  listenPort: number;
  connectedPeers: number;
  trustedPeers: number;
  lastError: string | null;
}

/** 网络里当前可见的设备。 */
export interface PeerView {
  deviceId: string;
  name: string;
  platform: Platform;
  address: string;           // "192.168.1.7:47711"
  fingerprint: string;
  trusted: boolean;          // 是否已在信任列表
  connected: boolean;        // 是否已建立 TLS 会话
  pairing: boolean;          // 是否正在配对
  lastSeen: number;          // Unix 毫秒
}

/** 已信任设备。 */
export interface TrustedDeviceView {
  deviceId: string;
  name: string;
  platform: Platform;
  fingerprint: string;
  trustedAt: number;
  online: boolean;
}

/** 等待用户决定的配对请求。 */
export interface PairingPrompt {
  deviceId: string;
  name: string;
  platform: Platform;
  fingerprint: string;
  address: string;
  requestedAt: number;
  direction: PairingDirection;
}

/** 历史条目（图片只有元数据，没有像素）。 */
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
      size: number;      // 字节
      width: number;
      height: number;
    };

/** 用户设置。 */
export interface SettingsView {
  deviceName: string;
  autoSync: boolean;          // 后台监听并自动同步
  syncText: boolean;
  syncImages: boolean;
  maxImageBytes: number;
  startMinimized: boolean;    // 桌面：启动即最小化到托盘
  launchAtLogin: boolean;     // 桌面：开机自启
  androidForegroundService: boolean; // Android：常驻前台服务
}

/** 发送结果。 */
export interface SendResult {
  id: string;
  delivered: number;          // 成功投递的在线设备数
}

/** 本机身份信息。 */
export interface IdentityView {
  deviceId: string;
  deviceName: string;
  platform: Platform;
  fingerprint: string;
  publicKey: string;          // hex
  certificatePem: string;     // 供用户核对/导出
}
```

---

## 3. 命令（`invoke`）

### 状态与列表

| 命令 | 参数 | 返回 |
| --- | --- | --- |
| `get_status` | — | `StatusView` |
| `list_peers` | — | `PeerView[]` |
| `list_trusted_devices` | — | `TrustedDeviceView[]` |
| `list_pairing_requests` | — | `PairingPrompt[]` |
| `get_history` | — | `ClipboardItemView[]` |
| `get_settings` | — | `SettingsView` |
| `get_identity` | — | `IdentityView` |

### 生命周期

| 命令 | 参数 | 返回 | 说明 |
| --- | --- | --- | --- |
| `start_engine` | — | `StatusView` | 幂等 |
| `stop_engine` | — | `StatusView` | 幂等 |
| `update_settings` | `{ patch: Partial<SettingsView> }` | `SettingsView` | 立即生效并持久化 |
| `set_device_name` | `{ name: string }` | `IdentityView` | 重新广播 mDNS |

### 配对与信任

| 命令 | 参数 | 返回 | 说明 |
| --- | --- | --- | --- |
| `request_pairing` | `{ deviceId: string }` | `void` | 主动向已发现设备发起配对 |
| `respond_pairing` | `{ deviceId: string, accept: boolean }` | `void` | 回应收到的配对请求 |
| `unpair_device` | `{ deviceId: string }` | `void` | 移出信任列表并断开 |

### 剪贴板

| 命令 | 参数 | 返回 | 说明 |
| --- | --- | --- | --- |
| `send_clipboard` | — | `SendResult` | **读取当前系统剪贴板并广播**（Android 通知栏「广播剪贴板」按钮走这条） |
| `send_text` | `{ content: string }` | `SendResult` | 直接广播给定文本 |
| `resend_history_item` | `{ id: string }` | `SendResult` | 重新发送历史条目 |
| `copy_history_item` | `{ id: string }` | `void` | 只写本机剪贴板，不发送 |
| `clear_history` | — | `void` | |
| `get_image_thumbnail` | `{ id: string, maxSize: number }` | `string` | 返回 **data URL**，已降采样 |

> `get_image_thumbnail` 是本项目里唯一使用 base64 的地方，并且只服务于本地 UI 缩略图。
> 网络上传输图片永远是原始 PNG 二进制分片（见 `crates/protocol/src/frame.rs`）。

### Android 专属

| 命令 | 参数 | 返回 | 说明 |
| --- | --- | --- | --- |
| `android_start_service` | — | `void` | 启动前台服务 |
| `android_stop_service` | — | `void` | 停止前台服务 |
| `android_service_running` | — | `boolean` | |
| `android_request_notification_permission` | — | `boolean` | Android 13+ 通知权限 |

在非 Android 平台调用这些命令会抛出 `"android commands are only available on Android"`。

---

## 4. 事件（`listen`）

前端只需在 `App.vue` 里挂一次 `useCoreEvents()`，它会把所有事件写进 Pinia store。

| 事件名 | 负载 | 触发时机 |
| --- | --- | --- |
| `clipmesh://status` | `StatusView` | 运行状态、在线数、错误变化 |
| `clipmesh://peers` | `PeerView[]` | 设备出现/消失/连接状态变化 |
| `clipmesh://trusted` | `TrustedDeviceView[]` | 信任列表变化 |
| `clipmesh://pairing-requests` | `PairingPrompt[]` | 有人请求配对，或请求被处理 |
| `clipmesh://history` | `ClipboardItemView[]` | 历史变化（最多 50 条，最新在前） |
| `clipmesh://clipboard-received` | `ClipboardItemView` | 收到远端剪贴板并已写入本机 |
| `clipmesh://clipboard-sent` | `{ item: ClipboardItemView, delivered: number }` | 本地内容已推送 |
| `clipmesh://error` | `string` | 非致命错误，用于 toast |

---

## 5. 平台与布局

两端 **共享**：类型、Pinia store、`invoke` 封装、通用组件、`useCoreEvents`。

两端 **不共享** 布局，各自实现：

| | Desktop (`apps/desktop/ui`) | Android (`apps/android/ui`) |
| --- | --- | --- |
| 布局 | 侧边导航 + 多栏 | 底部标签栏 + 单列 |
| 交互 | 鼠标悬停、右键、快捷键 | 大按钮、触摸目标 ≥48px |
| 视图 | Dashboard / Devices / History / Settings / Pairing | Home / Devices / History / Settings |
| 特殊 | 系统托盘、窗口控制 | 前台服务开关、广播按钮、通知权限 |

判定方式：`useDeviceType()` 依据 `platform` 与窗口宽度返回 `"desktop" | "mobile"`，
但**两个 app 各自静态挂载自己的 Layout**，不做运行时二选一，避免把两套布局都打进包。
