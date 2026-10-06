/**
 * 两个 app 的设置页文案（含语言选择器）。
 *
 * 桌面与 Android 的设置项并不完全一样，所以分开的键比共用的键多；
 * 真正共用的（同步开关、图片上限）只留一份。
 *
 * 语言选项本身是**自名**的：`中文` 与 `English` 在两张表里都写自己的语言，
 * 只有「系统」跟着界面语言走 —— 万一用户选错了语言，也总能在下拉框里
 * 认出自己那一项。
 */

import type { MessageTable } from "../types";

export const settings = {
  /* --- 页面框架 --- */
  "settings.title": { "zh-CN": "设置", en: "Settings" },
  "settings.subtitle": {
    "zh-CN": "改动会立即生效并写入本机配置。标记",
    en: "Changes take effect immediately and are saved to this device. Options marked",
  },
  "settings.subtitleZap": {
    "zh-CN": "的选项不需要重启应用。",
    en: "do not need an app restart.",
  },
  "settings.status.saving": { "zh-CN": "保存中…", en: "Saving…" },
  "settings.status.ready": { "zh-CN": "配置已就绪", en: "Config ready" },
  "settings.saveFailed": { "zh-CN": "设置未保存", en: "Settings not saved" },

  /* --- 语言 --- */
  "settings.language.label": { "zh-CN": "界面语言", en: "Language" },
  "settings.language.help": {
    "zh-CN": "「系统」跟随系统语言；选中文或 English 会覆盖它。切换立即生效，不用重启。",
    en: "System follows your OS language; choosing 中文 or English overrides it. Applied immediately, no restart.",
  },
  "settings.language.system": { "zh-CN": "系统", en: "System" },
  "settings.language.zh": { "zh-CN": "中文", en: "中文" },
  "settings.language.en": { "zh-CN": "English", en: "English" },

  /* --- 同步 --- */
  "settings.sync.title": { "zh-CN": "同步", en: "Sync" },
  "settings.sync.subtitle": {
    "zh-CN": "控制哪些内容会被自动同步",
    en: "Choose what gets synced automatically",
  },
  "settings.autoSync.label": { "zh-CN": "后台自动同步", en: "Background sync" },
  "settings.autoSync.description": {
    "zh-CN": "监听本机剪贴板，内容一变化就推送给已信任设备。",
    en: "Watches this device's clipboard and pushes every change to trusted devices.",
  },
  "settings.autoSync.descriptionMobile": {
    "zh-CN": "剪贴板变化时自动推送。",
    en: "Pushes clipboard changes automatically.",
  },
  "settings.syncText.label": { "zh-CN": "同步文本", en: "Sync text" },
  "settings.syncImages.label": { "zh-CN": "同步图片", en: "Sync images" },
  "settings.syncImages.description": {
    "zh-CN": "图片以原始 PNG 二进制分片传输，不走 Base64。",
    en: "Images are sent as raw PNG chunks, not Base64.",
  },
  "settings.syncImages.descriptionMobile": {
    "zh-CN": "移动网络下建议关掉，图片可能很大。",
    en: "Worth turning off on mobile data — images can be large.",
  },
  "settings.maxImageBytes.label": { "zh-CN": "图片大小上限", en: "Maximum image size" },
  "settings.maxImageBytes.hintBefore": { "zh-CN": "超过", en: "Images larger than" },
  "settings.maxImageBytes.hintAfter": {
    "zh-CN": "的图片会被跳过，并在日志里记一条。",
    en: "are skipped and written to the log.",
  },
  "settings.maxImageBytes.helpCurrent": {
    "zh-CN": "当前上限 {size}，超过的图片会被跳过。",
    en: "Current limit is {size}; larger images are skipped.",
  },

  /* --- 桌面专属 --- */
  "settings.desktop.title": { "zh-CN": "桌面行为", en: "Desktop behaviour" },
  "settings.desktop.subtitle": { "zh-CN": "窗口与开机启动", en: "Window and startup" },
  "settings.startMinimized.label": {
    "zh-CN": "启动后最小化到托盘",
    en: "Start minimised to the tray",
  },
  "settings.startMinimized.description": {
    "zh-CN": "开机自启时不弹出窗口，只在托盘里待命。",
    en: "Launches without showing a window — it waits in the tray instead.",
  },
  "settings.launchAtLogin.label": { "zh-CN": "开机自启", en: "Launch at login" },

  /* --- Android 专属 --- */
  "settings.foreground.title": { "zh-CN": "后台常驻", en: "Background service" },
  "settings.foreground.subtitle": {
    "zh-CN": "前台服务 + 通知",
    en: "Foreground service + notifications",
  },
  "settings.foreground.label": { "zh-CN": "常驻前台服务", en: "Keep the foreground service" },
  "settings.foreground.description": {
    "zh-CN": "保持后台运行，通知栏会显示常驻通知与「广播剪贴板」按钮。",
    en: "Keeps running in the background; the notification shade shows a persistent notification and a broadcast button.",
  },
  "settings.foreground.help": {
    "zh-CN":
      "这个开关跟着通知权限走：权限被拒绝时它是关的，前台服务也会停掉 —— 没有通知权限就没有常驻通知，也收不到远程剪贴板提醒。打开时会申请一次权限（系统里已经允许过就不再弹框）；被拒绝的话再点一次可以重新申请。",
    en: "This switch follows the notification permission: deny it and the switch reads off and the service stops — without the permission there is no persistent notification and no alert for remote clipboard items. Turning it on asks for the permission once (no dialog if it is already granted); if it was denied, tap again to ask a second time.",
  },
  "settings.foreground.noPermission.title": {
    "zh-CN": "没有通知权限",
    en: "No notification permission",
  },
  "settings.foreground.noPermission.description": {
    "zh-CN":
      "后台同步照常，但没有常驻通知，也收不到远程剪贴板的提醒。可以在系统设置里重新允许通知。",
    en: "Background sync still works, but there is no persistent notification and no alert for remote clipboard items. You can allow notifications again in system settings.",
  },
  "settings.foreground.started.title": {
    "zh-CN": "前台服务已启动",
    en: "Foreground service started",
  },
  "settings.foreground.started.description": {
    "zh-CN": "通知栏会显示常驻通知与「广播剪贴板」按钮。",
    en: "The notification shade now shows a persistent notification and a broadcast button.",
  },
  "settings.foreground.stopped": {
    "zh-CN": "前台服务已停止",
    en: "Foreground service stopped",
  },

  /* --- Android「关于」卡片 --- */
  "settings.about.title": { "zh-CN": "关于", en: "About" },
  "settings.about.mock": { "zh-CN": "浏览器 MOCK", en: "Browser MOCK" },
  "settings.about.tauri": { "zh-CN": "Tauri 运行时", en: "Tauri runtime" },
  "settings.about.body": {
    "zh-CN": "无中心服务器，设备之间直接通过 TLS 通信。当前监听端口 {port}。",
    en: "No central server — devices talk to each other directly over TLS. Listening on port {port}.",
  },
  "settings.about.licenseBefore": {
    "zh-CN": "以 MIT 许可证发布，全文见仓库根目录的",
    en: "Released under the MIT licence; the full text is in",
  },
  "settings.about.licenseAfter": { "zh-CN": "。", en: "." },
} satisfies MessageTable;
