/**
 * ClipMesh IPC 数据类型 —— 与 `docs/IPC.md` §2 逐字对应。
 *
 * 这个文件是冻结契约的 TS 侧镜像：字段名、联合类型、可空性都必须与 Rust 侧
 * `#[serde(rename_all = "camelCase")]` 的视图类型保持一致。
 * 改动这里之前请先改 `docs/IPC.md`。
 */

export type Platform = "windows" | "linux" | "macos" | "android" | "unknown";
export type ClipboardKind = "text" | "image";
export type PairingDirection = "incoming" | "outgoing";

/** 本机状态。 */
export interface StatusView {
  running: boolean; // 引擎（发现 + 监听）是否在运行
  autoSync: boolean; // 本地剪贴板变化是否自动推送
  deviceId: string;
  deviceName: string;
  platform: Platform;
  fingerprint: string; // 形如 "A1B2 C3D4 E5F6 ..."
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
  address: string; // "192.168.1.7:47711"
  fingerprint: string;
  trusted: boolean; // 是否已在信任列表
  connected: boolean; // 是否已建立 TLS 会话
  pairing: boolean; // 是否正在配对
  lastSeen: number; // Unix 毫秒
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
      mime: string; // "image/png"
      size: number; // 字节
      width: number;
      height: number;
    };

/** 用户设置。 */
export interface SettingsView {
  deviceName: string;
  autoSync: boolean; // 后台监听并自动同步
  syncText: boolean;
  syncImages: boolean;
  maxImageBytes: number;
  startMinimized: boolean; // 桌面：启动即最小化到托盘
  launchAtLogin: boolean; // 桌面：开机自启
  androidForegroundService: boolean; // Android：常驻前台服务
}

/** 发送结果。 */
export interface SendResult {
  id: string;
  delivered: number; // 成功投递的在线设备数
}

/** 本机身份信息。 */
export interface IdentityView {
  deviceId: string;
  deviceName: string;
  platform: Platform;
  fingerprint: string;
  publicKey: string; // hex
  certificatePem: string; // 供用户核对/导出
}

/* ------------------------------------------------------------------ *
 * 命令表（docs/IPC.md §3）
 * ------------------------------------------------------------------ */

/** `clipmesh://clipboard-sent` 的负载。 */
export interface ClipboardSentPayload {
  item: ClipboardItemView;
  delivered: number;
}

/**
 * 命令名 -> { 参数, 返回 } 的映射。`args: void` 表示该命令不接受参数。
 * 参数名保持 camelCase（Tauri 2 会把 JS 的 camelCase 映射到 Rust 的 snake_case）。
 */
export interface CommandMap {
  // 状态与列表
  get_status: { args: void; result: StatusView };
  list_peers: { args: void; result: PeerView[] };
  list_trusted_devices: { args: void; result: TrustedDeviceView[] };
  list_pairing_requests: { args: void; result: PairingPrompt[] };
  get_history: { args: void; result: ClipboardItemView[] };
  get_settings: { args: void; result: SettingsView };
  get_identity: { args: void; result: IdentityView };

  // 生命周期
  start_engine: { args: void; result: StatusView };
  stop_engine: { args: void; result: StatusView };
  clear_error: { args: void; result: StatusView };
  update_settings: { args: { patch: Partial<SettingsView> }; result: SettingsView };
  set_device_name: { args: { name: string }; result: IdentityView };

  // 配对与信任
  request_pairing: { args: { deviceId: string }; result: void };
  respond_pairing: { args: { deviceId: string; accept: boolean }; result: void };
  unpair_device: { args: { deviceId: string }; result: void };

  // 剪贴板
  send_clipboard: { args: void; result: SendResult };
  send_text: { args: { content: string }; result: SendResult };
  resend_history_item: { args: { id: string }; result: SendResult };
  copy_history_item: { args: { id: string }; result: void };
  clear_history: { args: void; result: void };
  get_image_thumbnail: { args: { id: string; maxSize: number }; result: string };

  // Android 专属
  android_start_service: { args: void; result: void };
  android_stop_service: { args: void; result: void };
  android_service_running: { args: void; result: boolean };
  android_notification_permission: { args: void; result: boolean };
  android_request_notification_permission: { args: void; result: boolean };
  android_leave_app: { args: void; result: void };
}

export type CommandName = keyof CommandMap;
/** 某个命令的参数类型。 */
export type Args<K extends CommandName> = CommandMap[K]["args"];
/** 某个命令的返回类型。 */
export type Result<K extends CommandName> = CommandMap[K]["result"];

/* ------------------------------------------------------------------ *
 * 事件表（docs/IPC.md §4）
 * ------------------------------------------------------------------ */

/** 事件名 -> 负载。事件是**快照**而不是增量。 */
export interface EventMap {
  "clipmesh://status": StatusView;
  "clipmesh://peers": PeerView[];
  "clipmesh://trusted": TrustedDeviceView[];
  "clipmesh://pairing-requests": PairingPrompt[];
  "clipmesh://history": ClipboardItemView[];
  "clipmesh://clipboard-received": ClipboardItemView;
  "clipmesh://clipboard-sent": ClipboardSentPayload;
  "clipmesh://error": string;
}

export type EventName = keyof EventMap;
/** 某个事件的负载类型。 */
export type EventPayload<K extends EventName> = EventMap[K];

/** 取消监听的函数（与 `@tauri-apps/api/event` 的 `UnlistenFn` 结构一致）。 */
export type UnlistenFn = () => void;
