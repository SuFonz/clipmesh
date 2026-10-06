/**
 * Android 端 `apps/android/ui/src` 的文案（`views/*`、`layouts/*`、`router.ts`）。
 *
 * 键名以 `android.` 开头，扁平点分命名（`android.<页面>.<用途>`）。
 * 每条都要给 `zh-CN` 与 `en` 两个值，缺一个就编译不过。
 *
 * `android.nav.*` 同时被路由的 `meta.title` 和底部标签栏使用，所以标题栏与
 * 标签栏永远一致。
 */

import type { MessageTable } from "../types";

export const android = {
  /* --- 外壳：标题栏 / 标签栏 / 顶栏动作 --- */
  "android.nav.aria": { "zh-CN": "主导航", en: "Main navigation" },
  "android.nav.home": { "zh-CN": "首页", en: "Home" },
  "android.nav.devices": { "zh-CN": "设备", en: "Devices" },
  "android.nav.history": { "zh-CN": "历史", en: "History" },
  "android.nav.settings": { "zh-CN": "设置", en: "Settings" },

  "android.shell.engineStopped": { "zh-CN": "引擎已停止", en: "Engine stopped" },
  "android.shell.subtitle": {
    "zh-CN": "{online} 台在线 · 已信任 {trusted}",
    en: "{online} online · {trusted} trusted",
  },
  "android.shell.pairingRequests": {
    "zh-CN": "{count} 个配对请求",
    en: "{count} pairing requests",
  },
  "android.shell.broadcast": { "zh-CN": "广播剪贴板", en: "Broadcast clipboard" },
  "android.shell.engineStopTitle": { "zh-CN": "停止引擎", en: "Stop the engine" },
  "android.shell.engineStartTitle": { "zh-CN": "启动引擎", en: "Start the engine" },
  "android.shell.toast.engineStopped": { "zh-CN": "引擎已停止", en: "Engine stopped" },
  "android.shell.toast.engineStarted": { "zh-CN": "引擎已启动", en: "Engine started" },
  "android.shell.toast.broadcast": {
    "zh-CN": "已广播到 {count} 台设备",
    en: "Broadcast to {count} devices",
  },
  "android.shell.toast.broadcastNoDevices": {
    "zh-CN": "没有在线设备，已存入历史",
    en: "No devices online — saved to history",
  },
  "android.shell.toast.broadcastFailed": { "zh-CN": "广播失败", en: "Broadcast failed" },

  /* --- 首页 --- */
  "android.home.thisDeviceSubtitle": {
    "zh-CN": "名称、身份与证书",
    en: "Name, identity and certificate",
  },
  "android.home.name.title": { "zh-CN": "本机名称", en: "Device name" },
  "android.home.name.subtitle": {
    "zh-CN": "同一网络里的其他设备会看到这个名字",
    en: "Other devices on the same network see this name",
  },
  "android.home.name.placeholder": { "zh-CN": "例如：我的 Pixel", en: "For example: My Pixel" },
  "android.home.name.save": { "zh-CN": "保存名称", en: "Save name" },
  "android.home.identity.title": { "zh-CN": "本机身份", en: "Device identity" },
  "android.home.identity.subtitle": {
    "zh-CN": "用于核对配对，不会上传到任何服务器",
    en: "Used to verify pairing — never uploaded to any server",
  },
  "android.home.online.manage": { "zh-CN": "管理", en: "Manage" },
  "android.home.online.empty.description": {
    "zh-CN": "确认对方也开着 ClipMesh，并且在同一网络里。",
    en: "Make sure the other device is running ClipMesh and is on the same network.",
  },

  /* --- 设备页 --- */
  "android.devices.discoveredCount": { "zh-CN": "发现 {count}", en: "Discovered {count}" },
  "android.devices.incoming.title": { "zh-CN": "配对请求", en: "Pairing requests" },
  "android.devices.incoming.subtitle": {
    "zh-CN": "{count} 个请求等待确认",
    en: "{count} requests waiting for you",
  },
  "android.devices.verifyFingerprint": {
    "zh-CN": "与对方屏幕核对指纹",
    en: "Check this against the other screen",
  },
  "android.devices.accept": { "zh-CN": "接受", en: "Accept" },
  "android.devices.reject": { "zh-CN": "拒绝", en: "Reject" },
  "android.devices.discovered.title": { "zh-CN": "发现的设备", en: "Discovered devices" },
  "android.devices.discovered.subtitle": {
    "zh-CN": "{count} 台等待配对",
    en: "{count} waiting to pair",
  },
  "android.devices.waiting": { "zh-CN": "等待中", en: "Waiting" },
  "android.devices.pair": { "zh-CN": "配对", en: "Pair" },
  "android.devices.discovered.empty.title": {
    "zh-CN": "没有发现新设备",
    en: "No new devices found",
  },
  "android.devices.trusted.title": { "zh-CN": "已信任的设备", en: "Trusted devices" },
  "android.devices.trusted.subtitle": {
    "zh-CN": "{count} 台 · {online} 台在线",
    en: "{count} total · {online} online",
  },
  "android.devices.trustedAt": { "zh-CN": "信任于 {date}", en: "Trusted {date}" },
  "android.devices.unpair": { "zh-CN": "解除", en: "Remove" },
  "android.devices.trusted.empty.title": {
    "zh-CN": "还没有信任任何设备",
    en: "No trusted devices yet",
  },
  "android.devices.trusted.empty.description": {
    "zh-CN": "先在上面找一台设备完成配对。",
    en: "Pair with one of the devices above first.",
  },
  "android.devices.unpairDialog.title": { "zh-CN": "解除信任？", en: "Remove trust?" },
  "android.devices.unpairDialog.message": {
    "zh-CN": "{name} 会被移出信任列表并断开，需要重新配对才能同步。",
    en: "{name} is removed from the trust list and disconnected; you have to pair again before syncing.",
  },
  "android.devices.unpairDialog.confirm": { "zh-CN": "解除信任", en: "Remove trust" },

  "android.devices.toast.pairRequested": { "zh-CN": "已发起配对", en: "Pairing request sent" },
  "android.devices.toast.pairRequestedDescription": {
    "zh-CN": "让对方核对指纹后接受。",
    en: "Ask the other side to check the fingerprint and accept.",
  },
  "android.devices.toast.pairFailed": { "zh-CN": "配对失败", en: "Pairing failed" },
  "android.devices.toast.trusted": { "zh-CN": "已信任 {name}", en: "Trusted {name}" },
  "android.devices.toast.rejected": { "zh-CN": "已拒绝 {name}", en: "Rejected {name}" },
  "android.devices.toast.unpaired": { "zh-CN": "已解除信任", en: "Trust removed" },
  "android.devices.toast.unpairFailed": {
    "zh-CN": "解除失败",
    en: "Could not remove trust",
  },

  /* --- 历史页 --- */
  "android.history.searchPlaceholder": { "zh-CN": "搜索历史", en: "Search history" },
  "android.history.clearTitle": { "zh-CN": "清空历史", en: "Clear history" },
  "android.history.filter.all": { "zh-CN": "全部", en: "All" },
  "android.history.filter.text": { "zh-CN": "文本", en: "Text" },
  "android.history.filter.image": { "zh-CN": "图片", en: "Images" },
  "android.history.emptyFiltered.title": { "zh-CN": "没有匹配的条目", en: "Nothing matches" },
  "android.history.emptyFiltered.description": {
    "zh-CN": "换个关键词试试。",
    en: "Try another keyword.",
  },
  "android.history.clearFilters": { "zh-CN": "清除筛选", en: "Clear filters" },
  "android.history.empty.title": { "zh-CN": "还没有历史", en: "No history yet" },
  "android.history.empty.description": {
    "zh-CN": "收到的剪贴板会自动出现在这里。",
    en: "Clipboard items you receive show up here automatically.",
  },
  "android.history.count": { "zh-CN": "共 {count} 条", en: "{count} items" },
  "android.history.confirm.title": { "zh-CN": "清空全部历史？", en: "Clear all history?" },
  "android.history.confirm.message": {
    "zh-CN": "将删除本机保存的 {count} 条记录，无法恢复。",
    en: "This deletes the {count} items stored on this device and cannot be undone.",
  },
  "android.history.confirm.confirm": { "zh-CN": "清空", en: "Clear" },

  "android.history.toast.copied": { "zh-CN": "已复制到剪贴板", en: "Copied to the clipboard" },
  "android.history.toast.resent": {
    "zh-CN": "已重发（{count} 台设备）",
    en: "Resent to {count} devices",
  },
  "android.history.toast.resendFailed": { "zh-CN": "重发失败", en: "Resend failed" },
  "android.history.toast.cleared": { "zh-CN": "历史已清空", en: "History cleared" },
  "android.history.toast.clearFailed": { "zh-CN": "清空失败", en: "Could not clear history" },
} satisfies MessageTable;
