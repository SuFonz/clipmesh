/**
 * `@clipmesh/ui-core` —— Desktop 与 Android 两个 UI 共享的一切：
 * 类型、IPC 封装、Pinia store、composables、通用组件。
 *
 * 布局**不在这里**：桌面用 `apps/desktop/ui/src/layouts/DesktopLayout.vue`，
 * Android 用 `apps/android/ui/src/layouts/MobileLayout.vue`，各自静态挂载。
 */

// 全局样式（设计令牌 + 重置）随包一起带上，两个 app 不需要各自 import
import "./theme.css";

/* --- IPC 契约类型 --- */
export type {
  Args,
  ClipboardItemView,
  ClipboardKind,
  ClipboardSentPayload,
  CommandMap,
  CommandName,
  EventMap,
  EventName,
  EventPayload,
  IdentityView,
  PairingDirection,
  PairingPrompt,
  PeerView,
  Platform,
  Result,
  SendResult,
  SettingsView,
  StatusView,
  TrustedDeviceView,
  UnlistenFn,
} from "./types";

/* --- transport / commands --- */
export { call, configureMock, isMock, isTauri, listenEvent, resetMock, stopMock } from "./api/transport";
export * from "./api/commands";

/* --- stores --- */
export { toMessage, useStatusStore } from "./stores/status";
export { usePeersStore } from "./stores/peers";
export { useTrustedStore } from "./stores/trusted";
export { usePairingStore } from "./stores/pairing";
export { useHistoryStore, type HistoryFilter } from "./stores/history";
export { MAX_IMAGE_BYTES_OPTIONS, useSettingsStore } from "./stores/settings";
export { useIdentityStore } from "./stores/identity";

/* --- composables --- */
export { useAppVersion } from "./composables/useAppVersion";
export { useCoreEvents, type CoreEventsHandle } from "./composables/useCoreEvents";
export { MOBILE_BREAKPOINT, useDeviceType, type DeviceType, type DeviceTypeInfo } from "./composables/useDeviceType";
export { useRelativeTime, type RelativeTime } from "./composables/useRelativeTime";
export { useToast, type Toast, type ToastTone } from "./composables/useToast";

/* --- 工具 --- */
export {
  formatBytes,
  formatClock,
  formatDateTime,
  formatMaxImageBytes,
  formatRelative,
  groupFingerprint,
  platformLabel,
  shortFingerprint,
  shortId,
  summarizeItem,
  truncate,
} from "./utils/format";
export { copyToClipboard, downloadTextFile } from "./utils/dom";

/* --- 通用组件 --- */
export { default as AppButton } from "./components/AppButton.vue";
export { default as AppCard } from "./components/AppCard.vue";
export { default as AppIcon } from "./components/AppIcon.vue";
export { default as AppToggle } from "./components/AppToggle.vue";
export { default as ClipboardItemCard } from "./components/ClipboardItemCard.vue";
export { default as ConfirmDialog } from "./components/ConfirmDialog.vue";
export { default as DeviceCard } from "./components/DeviceCard.vue";
export { default as EmptyState } from "./components/EmptyState.vue";
export { default as FingerprintBadge } from "./components/FingerprintBadge.vue";
export { default as StatusPill } from "./components/StatusPill.vue";
export { default as ToastHost } from "./components/ToastHost.vue";
export { ICONS, PLATFORM_ICONS, type IconDefinition, type IconName } from "./components/icons";
