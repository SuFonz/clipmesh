/**
 * 桌面端 `apps/desktop/ui/src` 的文案（`views/*`、`layouts/*`、`router.ts`）。
 *
 * 键名以 `desktop.` 开头，扁平点分命名（`desktop.<页面>.<用途>`）。
 * 每条都要给 `zh-CN` 与 `en` 两个值，缺一个就编译不过。
 *
 * `desktop.nav.*` 同时被路由的 `meta.title` 和侧边导航使用，所以标题栏与
 * 导航标签永远一致。
 */

import type { MessageTable } from "../types";

export const desktop = {
  /* --- 外壳：品牌 / 导航 / 状态条 --- */
  "desktop.brand.tagline": { "zh-CN": "P2P 剪贴板同步", en: "P2P clipboard sync" },
  "desktop.nav.aria": { "zh-CN": "主导航", en: "Main navigation" },
  "desktop.nav.home": { "zh-CN": "首页", en: "Home" },
  "desktop.nav.homeHint": { "zh-CN": "本机与在线设备", en: "This device and online devices" },
  "desktop.nav.devices": { "zh-CN": "设备", en: "Devices" },
  "desktop.nav.devicesHint": {
    "zh-CN": "配对请求 / 发现 / 信任的设备",
    en: "Pairing requests, discovered and trusted devices",
  },
  "desktop.nav.history": { "zh-CN": "历史", en: "History" },
  "desktop.nav.historyHint": { "zh-CN": "最近的剪贴板内容", en: "Recent clipboard items" },
  "desktop.nav.settings": { "zh-CN": "设置", en: "Settings" },
  "desktop.nav.settingsHint": { "zh-CN": "同步与身份", en: "Sync and identity" },
  "desktop.nav.about": { "zh-CN": "关于", en: "About" },
  "desktop.nav.aboutHint": { "zh-CN": "版本与运行信息", en: "Version and runtime info" },

  "desktop.status.running": { "zh-CN": "运行中", en: "Running" },
  "desktop.status.stopped": { "zh-CN": "已停止", en: "Stopped" },
  "desktop.status.mockTitle": {
    "zh-CN": "没有检测到 Tauri 运行时，正在使用内存 mock 数据",
    en: "No Tauri runtime detected — using in-memory mock data",
  },

  "desktop.statusbar.online": { "zh-CN": "在线", en: "Online" },
  "desktop.statusbar.trusted": { "zh-CN": "已信任", en: "Trusted" },
  "desktop.statusbar.discovered": { "zh-CN": "发现", en: "Discovered" },
  "desktop.statusbar.portTitle": {
    "zh-CN": "mDNS 监听端口 {port}",
    en: "mDNS listening port {port}",
  },

  "desktop.engine.stopTitle": { "zh-CN": "停止引擎", en: "Stop the engine" },
  "desktop.engine.startTitle": { "zh-CN": "启动引擎", en: "Start the engine" },
  "desktop.engine.stop": { "zh-CN": "停止", en: "Stop" },
  "desktop.engine.start": { "zh-CN": "启动", en: "Start" },
  "desktop.engine.stoppedToast": { "zh-CN": "引擎已停止", en: "Engine stopped" },
  "desktop.engine.stoppedDescription": {
    "zh-CN": "不再监听剪贴板，也不再响应局域网请求。",
    en: "It no longer watches the clipboard or answers LAN requests.",
  },
  "desktop.engine.startedToast": { "zh-CN": "引擎已启动", en: "Engine started" },
  "desktop.engine.startedDescription": {
    "zh-CN": "正在通过 mDNS 广播并监听剪贴板。",
    en: "Broadcasting over mDNS and watching the clipboard.",
  },

  /* --- 首页 --- */
  "desktop.home.engineRunning": { "zh-CN": "引擎运行中", en: "engine running" },
  "desktop.home.engineStopped": { "zh-CN": "引擎已停止", en: "engine stopped" },
  "desktop.home.devicesOnline": { "zh-CN": "{count} 台设备在线", en: "{count} devices online" },
  "desktop.home.discovered": { "zh-CN": "发现 {count}", en: "Discovered {count}" },
  "desktop.home.sendClipboard": { "zh-CN": "发送剪贴板", en: "Send clipboard" },
  "desktop.home.dismissError": { "zh-CN": "知道了", en: "Got it" },
  "desktop.home.thisDeviceSubtitle": {
    "zh-CN": "身份、证书与运行状态",
    en: "Identity, certificate and runtime",
  },

  "desktop.home.identity.title": { "zh-CN": "本机身份", en: "Device identity" },
  "desktop.home.identity.subtitle": {
    "zh-CN": "长期保存，卸载重装会重新生成",
    en: "Kept long-term; reinstalling generates a new one",
  },
  "desktop.home.identity.deviceNamePlaceholder": {
    "zh-CN": "例如：书房的台式机",
    en: "For example: Study desktop",
  },

  "desktop.home.runtime.title": { "zh-CN": "运行状态", en: "Runtime" },
  "desktop.home.runtime.subtitle": {
    "zh-CN": "监听端口与同步情况",
    en: "Listening port and sync state",
  },
  "desktop.home.runtime.autoSyncOn": { "zh-CN": "已开启", en: "On" },
  "desktop.home.runtime.autoSyncOff": { "zh-CN": "已关闭", en: "Off" },
  "desktop.home.runtime.trustedDevices": { "zh-CN": "已信任设备", en: "Trusted devices" },
  "desktop.home.runtime.deviceCount": { "zh-CN": "{count} 台", en: "{count}" },
  "desktop.home.runtime.settingsHintBefore": {
    "zh-CN": "自动同步、图片大小上限等开关在",
    en: "Switches such as background sync and the image size limit live in ",
  },
  "desktop.home.runtime.settingsHintAfter": { "zh-CN": "里调整。", en: "." },

  "desktop.home.online.manage": { "zh-CN": "管理设备", en: "Manage devices" },
  "desktop.home.online.empty.title": { "zh-CN": "当前没有在线设备", en: "No devices online" },
  "desktop.home.online.empty.description": {
    "zh-CN": "同一局域网里完成配对的设备上线后，会自动出现在这里。",
    en: "Paired devices on the same LAN show up here as soon as they come online.",
  },

  "desktop.home.toast.engineStartedDescription": {
    "zh-CN": "正在监听剪贴板并接受局域网连接。",
    en: "Watching the clipboard and accepting LAN connections.",
  },
  "desktop.home.toast.sent": { "zh-CN": "已发送剪贴板", en: "Clipboard sent" },
  "desktop.home.toast.sentDescription": {
    "zh-CN": "投递到 {count} 台在线设备",
    en: "Delivered to {count} online devices",
  },
  "desktop.home.toast.sendFailed": { "zh-CN": "发送失败", en: "Send failed" },
  "desktop.home.toast.clearErrorFailed": {
    "zh-CN": "无法清除这条错误",
    en: "Could not clear this error",
  },

  /* --- 设备页 --- */
  "desktop.devices.subtitleBefore": { "zh-CN": "本机在", en: "This device listens on" },
  "desktop.devices.subtitleAfter": {
    "zh-CN": "端口监听，通过 mDNS 发现同一局域网内的其他 ClipMesh 节点。发现 ≠ 信任，必须先配对。",
    en: "and discovers other ClipMesh nodes on the same LAN over mDNS. Discovery is not trust — pair first.",
  },
  "desktop.devices.searchPlaceholder": {
    "zh-CN": "按名称 / 指纹 / 地址筛选",
    en: "Filter by name, fingerprint or address",
  },
  "desktop.devices.incoming.title": { "zh-CN": "配对请求", en: "Pairing requests" },
  "desktop.devices.incoming.subtitle": {
    "zh-CN": "{count} 个请求等待确认",
    en: "{count} requests waiting for you",
  },
  "desktop.devices.verifyFingerprint": {
    "zh-CN": "与对方屏幕核对指纹",
    en: "Check this against the other screen",
  },
  "desktop.devices.accept": { "zh-CN": "接受", en: "Accept" },
  "desktop.devices.reject": { "zh-CN": "拒绝", en: "Reject" },
  "desktop.devices.outgoing.title": { "zh-CN": "我发出的请求", en: "Requests you sent" },
  "desktop.devices.outgoing.subtitle": {
    "zh-CN": "{count} 个请求等待对方确认",
    en: "{count} requests waiting for the other side",
  },
  "desktop.devices.discovered.title": { "zh-CN": "发现的设备", en: "Discovered devices" },
  "desktop.devices.discovered.subtitle": {
    "zh-CN": "{count} 台等待配对",
    en: "{count} waiting to pair",
  },
  "desktop.devices.waiting": { "zh-CN": "等待对方确认", en: "Waiting for the other side" },
  "desktop.devices.pair": { "zh-CN": "配对", en: "Pair" },
  "desktop.devices.discovered.empty.title": {
    "zh-CN": "没有待配对的设备",
    en: "No devices waiting to pair",
  },
  "desktop.devices.discovered.empty.description": {
    "zh-CN": "同一局域网内的新设备会自动出现在这里。",
    en: "New devices on the same LAN show up here automatically.",
  },
  "desktop.devices.trusted.title": { "zh-CN": "已信任的设备", en: "Trusted devices" },
  "desktop.devices.trusted.subtitle": {
    "zh-CN": "{count} 台 · {online} 台在线",
    en: "{count} total · {online} online",
  },
  "desktop.devices.unpair": { "zh-CN": "解除", en: "Remove" },
  "desktop.devices.trusted.empty.title": {
    "zh-CN": "还没有信任任何设备",
    en: "No trusted devices yet",
  },
  "desktop.devices.trusted.empty.description": {
    "zh-CN": "在左边找到设备并完成一次配对，之后就能互相同步剪贴板。",
    en: "Find a device on the left and pair once — after that you can sync clipboards both ways.",
  },
  "desktop.devices.unpairDialog.title": { "zh-CN": "解除信任？", en: "Remove trust?" },
  "desktop.devices.unpairDialog.message": {
    "zh-CN": "{name} 会被移出信任列表并断开当前会话，需要重新配对才能同步。",
    en: "{name} is removed from the trust list and the current session is dropped; you have to pair again before syncing.",
  },
  "desktop.devices.unpairDialog.confirm": { "zh-CN": "解除信任", en: "Remove trust" },

  "desktop.devices.toast.refreshed": { "zh-CN": "已刷新设备列表", en: "Device list refreshed" },
  "desktop.devices.toast.pairRequested": { "zh-CN": "已发送配对请求", en: "Pairing request sent" },
  "desktop.devices.toast.pairRequestedDescription": {
    "zh-CN": "等待对方在其设备上确认指纹。",
    en: "Waiting for the other side to confirm the fingerprint on their device.",
  },
  "desktop.devices.toast.pairFailed": { "zh-CN": "配对失败", en: "Pairing failed" },
  "desktop.devices.toast.paired": { "zh-CN": "已接受配对", en: "Pairing accepted" },
  "desktop.devices.toast.pairedDescription": {
    "zh-CN": "{name} 现在可以接收你的剪贴板了。",
    en: "{name} can now receive your clipboard.",
  },
  "desktop.devices.toast.rejected": { "zh-CN": "已拒绝配对", en: "Pairing rejected" },
  "desktop.devices.toast.rejectedDescription": {
    "zh-CN": "{name} 不会收到任何内容。",
    en: "{name} will receive nothing.",
  },
  "desktop.devices.toast.cancelled": { "zh-CN": "已取消配对请求", en: "Pairing request cancelled" },
  "desktop.devices.toast.cancelledDescription": {
    "zh-CN": "{name} 不会再收到这次请求。",
    en: "{name} will not see this request again.",
  },
  "desktop.devices.toast.cancelFailed": { "zh-CN": "取消失败", en: "Cancel failed" },
  "desktop.devices.toast.unpaired": { "zh-CN": "已解除信任", en: "Trust removed" },
  "desktop.devices.toast.unpairedDescription": {
    "zh-CN": "{name} 需要重新配对才能同步。",
    en: "{name} has to pair again before it can sync.",
  },
  "desktop.devices.toast.unpairFailed": { "zh-CN": "解除失败", en: "Could not remove trust" },

  /* --- 历史页 --- */
  "desktop.history.subtitle": {
    "zh-CN": "最多保留 50 条，最新在前。图片只保存元数据，缩略图是在本机现取的。",
    en: "Keeps the latest 50 items, newest first. Images store metadata only; thumbnails are rendered locally.",
  },
  "desktop.history.count": { "zh-CN": "共 {count} 条", en: "{count} items" },
  "desktop.history.clear": { "zh-CN": "清除剪贴板", en: "Clear clipboard" },
  "desktop.history.searchPlaceholder": {
    "zh-CN": "搜索文本内容或图片尺寸",
    en: "Search text content or image size",
  },
  "desktop.history.filterAria": { "zh-CN": "内容类型", en: "Content type" },
  "desktop.history.filter.all": { "zh-CN": "全部", en: "All" },
  "desktop.history.filter.text": { "zh-CN": "文本", en: "Text" },
  "desktop.history.filter.image": { "zh-CN": "图片", en: "Images" },
  "desktop.history.emptyFiltered.title": { "zh-CN": "没有匹配的条目", en: "Nothing matches" },
  "desktop.history.emptyFiltered.description": {
    "zh-CN": "换个关键词，或者把筛选切回「全部」。",
    en: "Try another keyword, or switch the filter back to “All”.",
  },
  "desktop.history.clearFilters": { "zh-CN": "清除筛选", en: "Clear filters" },
  "desktop.history.empty.title": { "zh-CN": "还没有任何历史", en: "No history yet" },
  "desktop.history.empty.description": {
    "zh-CN": "开启自动同步，其他设备同步过来的内容会自动出现在这里。",
    en: "Turn on background sync — anything other devices send shows up here automatically.",
  },
  "desktop.history.confirm.title": { "zh-CN": "清除剪贴板历史？", en: "Clear clipboard history?" },
  "desktop.history.confirm.message": {
    "zh-CN":
      "将删除本机保存的 {count} 条记录（包含图片元数据），此操作不可恢复。已经同步到其他设备的内容不受影响。",
    en: "This deletes the {count} items stored on this device (including image metadata) and cannot be undone. Anything already synced to other devices is unaffected.",
  },
  "desktop.history.confirm.confirm": { "zh-CN": "清除", en: "Clear" },
  "desktop.history.toast.copied": {
    "zh-CN": "已复制到本机剪贴板",
    en: "Copied to this device's clipboard",
  },
  "desktop.history.toast.resent": {
    "zh-CN": "已重新发送到 {count} 台设备",
    en: "Resent to {count} devices",
  },
  "desktop.history.toast.noOnlineDevicesDescription": {
    "zh-CN": "内容仍在历史里，等设备上线再试。",
    en: "The item is still in your history — try again once a device is online.",
  },
  "desktop.history.toast.resendFailed": { "zh-CN": "重发失败", en: "Resend failed" },
  "desktop.history.toast.cleared": { "zh-CN": "历史已清空", en: "History cleared" },
  "desktop.history.toast.clearFailed": { "zh-CN": "清空失败", en: "Could not clear history" },

  /* --- 关于页 --- */
  "desktop.about.subtitle": {
    "zh-CN": "应用版本、运行环境，以及本机此刻的监听情况。",
    en: "App version, runtime environment, and what this device is listening on right now.",
  },
  "desktop.about.version": { "zh-CN": "版本 {version}", en: "Version {version}" },
  "desktop.about.mockVersionNote": {
    "zh-CN": "浏览器 MOCK 运行时没有 Tauri 宿主，读不到打包时的版本号，所以这里显示占位符。",
    en: "The browser MOCK runtime has no Tauri host, so the packaged version number cannot be read and a placeholder is shown instead.",
  },
  "desktop.about.runtimeSubtitle": {
    "zh-CN": "本机身份与监听端口",
    en: "This device's identity and listening port",
  },
  "desktop.about.portNoteBefore": {
    "zh-CN":
      "端口优先用 47711（netstat 里好认，也方便写防火墙规则），被别的程序占用时自动换成系统分配的临时端口，真正算数的是 mDNS 广播出去的那个。设备名和完整证书在",
    en: "Port 47711 is preferred (easy to spot in netstat, easy to write firewall rules for); if something else holds it, the app falls back to an OS-assigned port — what counts is the one announced over mDNS. The device name and the full certificate live in ",
  },
  "desktop.about.portNoteAfter": { "zh-CN": "的「本机身份」里。", en: " under “Device identity”." },
  "desktop.about.security.title": { "zh-CN": "安全", en: "Security" },
  "desktop.about.security.subtitle": { "zh-CN": "配对才是信任边界", en: "Pairing is the trust boundary" },
  "desktop.about.security.fact1": {
    "zh-CN": "每台设备持有自己的 Ed25519 密钥与自签证书，通信全程走 TLS 1.3 双向认证加密。",
    en: "Every device holds its own Ed25519 key and self-signed certificate; all traffic is mutually authenticated and encrypted with TLS 1.3.",
  },
  "desktop.about.security.fact2": {
    "zh-CN": "发现不等于信任：必须在两台设备上对照证书指纹，并手动接受配对。",
    en: "Discovery is not trust: you must compare the certificate fingerprint on both devices and accept the pairing by hand.",
  },
  "desktop.about.security.fact3": {
    "zh-CN": "未配对的设备只能发配对请求，其余消息一律丢弃。",
    en: "An unpaired device may only send pairing requests; everything else is dropped.",
  },
} satisfies MessageTable;
