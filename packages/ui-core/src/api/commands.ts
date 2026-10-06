/**
 * `docs/IPC.md` §3 里每一个 `invoke` 的类型化封装。
 *
 * 视图层不应该直接写字符串命令名 —— 用这里的函数，参数名和返回类型都由
 * `CommandMap` 保证与 Rust 侧一致（参数 camelCase，返回 camelCase）。
 */

import { call } from "./transport";
import type {
  ClipboardItemView,
  IdentityView,
  PairingPrompt,
  PeerView,
  SendResult,
  SettingsView,
  StatusView,
  TrustedDeviceView,
} from "../types";

/* --- 状态与列表 --- */

export function getStatus(): Promise<StatusView> {
  return call("get_status");
}

export function listPeers(): Promise<PeerView[]> {
  return call("list_peers");
}

export function listTrustedDevices(): Promise<TrustedDeviceView[]> {
  return call("list_trusted_devices");
}

export function listPairingRequests(): Promise<PairingPrompt[]> {
  return call("list_pairing_requests");
}

export function getHistory(): Promise<ClipboardItemView[]> {
  return call("get_history");
}

export function getSettings(): Promise<SettingsView> {
  return call("get_settings");
}

export function getIdentity(): Promise<IdentityView> {
  return call("get_identity");
}

/* --- 生命周期 --- */

/** 启动引擎（幂等）。 */
export function startEngine(): Promise<StatusView> {
  return call("start_engine");
}

/** 停止引擎（幂等）。 */
export function stopEngine(): Promise<StatusView> {
  return call("stop_engine");
}

/**
 * 清掉引擎记住的那条错误，并返回新的状态快照。
 *
 * 关闭错误横幅必须走这里：横幅显示的是后端快照里的 `lastError`，
 * 只清前端本地变量的话，下一条 `clipmesh://status` 会把它原样带回来。
 */
export function clearError(): Promise<StatusView> {
  return call("clear_error");
}

/** 局部更新设置，立即生效并持久化。 */
export function updateSettings(patch: Partial<SettingsView>): Promise<SettingsView> {
  return call("update_settings", { patch });
}

/** 改设备名，并重新广播 mDNS。 */
export function setDeviceName(name: string): Promise<IdentityView> {
  return call("set_device_name", { name });
}

/* --- 配对与信任 --- */

export function requestPairing(deviceId: string): Promise<void> {
  return call("request_pairing", { deviceId });
}

export function respondPairing(deviceId: string, accept: boolean): Promise<void> {
  return call("respond_pairing", { deviceId, accept });
}

export function unpairDevice(deviceId: string): Promise<void> {
  return call("unpair_device", { deviceId });
}

/* --- 剪贴板 --- */

/** 读取当前系统剪贴板并广播。 */
export function sendClipboard(): Promise<SendResult> {
  return call("send_clipboard");
}

/** 直接广播给定文本。 */
export function sendText(content: string): Promise<SendResult> {
  return call("send_text", { content });
}

export function resendHistoryItem(id: string): Promise<SendResult> {
  return call("resend_history_item", { id });
}

/** 只写本机剪贴板，不发送。 */
export function copyHistoryItem(id: string): Promise<void> {
  return call("copy_history_item", { id });
}

export function clearHistory(): Promise<void> {
  return call("clear_history");
}

/** 取历史图片的本地缩略图（data URL，已降采样）。 */
export function getImageThumbnail(id: string, maxSize = 320): Promise<string> {
  return call("get_image_thumbnail", { id, maxSize });
}

/* --- Android 专属（非 Android 平台会抛错） --- */

export function androidStartService(): Promise<void> {
  return call("android_start_service");
}

export function androidStopService(): Promise<void> {
  return call("android_stop_service");
}

export function androidServiceRunning(): Promise<boolean> {
  return call("android_service_running");
}

export function androidRequestNotificationPermission(): Promise<boolean> {
  return call("android_request_notification_permission");
}

/**
 * 把应用退回后台。
 *
 * 通知栏那颗「广播剪贴板」默认走透明 Activity，不显示任何界面；只有它读不到剪贴板
 * 时才会把真正的 Activity 拉到前台，这时由 Rust 侧在广播成功后调用同一个原生方法把
 * 用户送回去。这个封装留给界面自己需要离开时用。
 * 非 Android 平台会抛错，调用方自己吞掉即可。
 */
export function androidLeaveApp(): Promise<void> {
  return call("android_leave_app");
}

/** 把上面这一组命令聚在一起，方便在组件里 `import { clipboard } from ...`。 */
export const clipboardApi = {
  sendClipboard,
  sendText,
  resendHistoryItem,
  copyHistoryItem,
  clearHistory,
  getImageThumbnail,
} as const;

export const deviceApi = {
  listPeers,
  listTrustedDevices,
  listPairingRequests,
  requestPairing,
  respondPairing,
  unpairDevice,
} as const;

export const androidApi = {
  startService: androidStartService,
  stopService: androidStopService,
  isServiceRunning: androidServiceRunning,
  requestNotificationPermission: androidRequestNotificationPermission,
  leaveApp: androidLeaveApp,
} as const;
