/**
 * `packages/ui-core/src/components/*` 的文案。
 *
 * 键名以 `components.` 开头，扁平点分命名（`components.<组件>.<用途>`）。
 * 每条都要给 `zh-CN` 与 `en` 两个值，缺一个就编译不过。
 *
 * 视图通过 prop 传进来的文字不在这里 —— 那些归各自的页面表。
 * 这里只有组件自己的默认值、`aria-label`、`title` 与组件内部产生的 toast。
 */

import type { MessageTable } from "../types";

export const components = {
  /* --- ClipboardItemCard --- */
  "components.clipboardItem.imageAlt": {
    "zh-CN": "剪贴板图片 {width}×{height}",
    en: "Clipboard image {width}×{height}",
  },
  "components.clipboardItem.imageAltPlain": { "zh-CN": "剪贴板图片", en: "Clipboard image" },
  "components.clipboardItem.new": { "zh-CN": "新", en: "NEW" },
  "components.clipboardItem.collapse": { "zh-CN": "收起", en: "Collapse" },
  "components.clipboardItem.expand": {
    "zh-CN": "展开全部（{count} 字符）",
    en: "Show all ({count} characters)",
  },
  "components.clipboardItem.thumbnailLoading": {
    "zh-CN": "缩略图加载中…",
    en: "Loading thumbnail…",
  },
  "components.clipboardItem.copyTitle": {
    "zh-CN": "复制到本机剪贴板（不发送）",
    en: "Copy to this device's clipboard (does not send)",
  },
  "components.clipboardItem.copy": { "zh-CN": "复制", en: "Copy" },
  "components.clipboardItem.shareTitle": {
    "zh-CN": "用系统分享面板发给其他应用",
    en: "Share with another app through the system share sheet",
  },
  "components.clipboardItem.share": { "zh-CN": "分享", en: "Share" },
  "components.clipboardItem.resendTitle": {
    "zh-CN": "重新发送到所有在线设备",
    en: "Send again to every online device",
  },
  "components.clipboardItem.resend": { "zh-CN": "重发", en: "Resend" },

  /* --- ConfirmDialog --- */
  /* 默认按钮文案用 `common.confirm` / `common.cancel`（通用词，不另立一份）。 */

  /* --- DeviceCard --- */
  "components.device.pairing": { "zh-CN": "配对中", en: "Pairing" },
  "components.device.connected": { "zh-CN": "已连接", en: "Connected" },
  "components.device.offline": { "zh-CN": "离线", en: "Offline" },
  "components.device.discovered": { "zh-CN": "已发现", en: "Discovered" },
  "components.device.trusted": { "zh-CN": "已信任", en: "Trusted" },

  /* --- FingerprintBadge --- */
  "components.fingerprint.copyTitle": {
    "zh-CN": "复制完整指纹",
    en: "Copy the full fingerprint",
  },
  "components.fingerprint.copied": { "zh-CN": "已复制", en: "Copied" },
  "components.fingerprint.copiedToast": { "zh-CN": "指纹已复制", en: "Fingerprint copied" },
  "components.fingerprint.copyUnavailable": {
    "zh-CN": "当前环境不允许访问剪贴板",
    en: "This environment does not allow clipboard access",
  },

  /* --- ToastHost --- */
  "components.toast.close": { "zh-CN": "关闭", en: "Close" },
} satisfies MessageTable;
