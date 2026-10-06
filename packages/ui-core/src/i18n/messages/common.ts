/**
 * 通用文案：跨页面、跨平台复用的词、store 的兜底值、时间格式、全局 toast。
 *
 * 键名扁平点分。只收「两个 app 都要用」的东西 —— 只在一个页面出现的文案放各自的表。
 */

import type { MessageTable } from "../types";

export const common = {
  /* --- 兜底值（store 里拿不到数据时显示） --- */
  "common.thisDevice": { "zh-CN": "本机", en: "This device" },
  "common.unknownDevice": { "zh-CN": "未知设备", en: "Unknown device" },
  "common.unknownError": { "zh-CN": "未知错误", en: "Unknown error" },
  "common.actionFailed": { "zh-CN": "操作失败", en: "Action failed" },
  "common.platformUnknown": { "zh-CN": "未知平台", en: "Unknown platform" },
  "common.mockBadge": { "zh-CN": "MOCK 数据", en: "MOCK data" },

  /* --- 通用动作 --- */
  "common.confirm": { "zh-CN": "确认", en: "Confirm" },
  "common.cancel": { "zh-CN": "取消", en: "Cancel" },
  "common.save": { "zh-CN": "保存", en: "Save" },
  "common.refresh": { "zh-CN": "刷新", en: "Refresh" },
  "common.refreshFailed": { "zh-CN": "刷新失败", en: "Refresh failed" },
  "common.copyFailed": { "zh-CN": "复制失败", en: "Copy failed" },

  /* --- 术语（多个页面复用） --- */
  "common.deviceName": { "zh-CN": "设备名", en: "Device name" },
  "common.platform": { "zh-CN": "平台", en: "Platform" },
  "common.engine": { "zh-CN": "引擎", en: "Engine" },
  "common.listenPort": { "zh-CN": "监听端口", en: "Listening port" },
  "common.certificateFingerprint": { "zh-CN": "证书指纹", en: "Certificate fingerprint" },
  "common.autoSync": { "zh-CN": "自动同步", en: "Auto sync" },
  "common.onlineDevices": { "zh-CN": "在线设备", en: "Online devices" },
  "common.onlineDevicesSubtitle": {
    "zh-CN": "{online} 台已连接 · {discovered} 台被发现",
    en: "{online} connected · {discovered} discovered",
  },
  "common.noOnlineDevices": { "zh-CN": "没有在线设备", en: "No devices online" },

  /* --- 本机身份（首页两个平台共用同一套字段） --- */
  "common.deviceId": { "zh-CN": "设备 ID", en: "Device ID" },
  "common.publicKey": { "zh-CN": "公钥", en: "Public key" },
  "common.certificate": { "zh-CN": "设备证书（PEM）", en: "Device certificate (PEM)" },
  "common.exportCertificate": { "zh-CN": "导出证书", en: "Export certificate" },
  "common.copyDeviceId": { "zh-CN": "复制设备 ID", en: "Copy device ID" },
  "common.copyPublicKey": { "zh-CN": "复制公钥", en: "Copy public key" },
  "common.copiedDeviceId": { "zh-CN": "设备 ID 已复制", en: "Device ID copied" },
  "common.copiedPublicKey": { "zh-CN": "公钥已复制", en: "Public key copied" },
  "common.fingerprintNote": {
    "zh-CN": "对方应该能在自己的设备上看到同一串指纹。指纹不同 = 有人在中间，别继续。",
    en: "The other side should see the same fingerprint on their device. Different fingerprints mean someone is in the middle — stop.",
  },
  "common.identityLoading": { "zh-CN": "正在读取身份信息…", en: "Reading identity…" },
  "common.currentName": { "zh-CN": "当前生效：", en: "Currently in effect: " },
  "common.currentNameAfter": {
    "zh-CN": "，改名后会重新广播 mDNS。",
    en: ". Renaming re-announces over mDNS.",
  },

  /* --- 相对时间 --- */
  "common.time.justNow": { "zh-CN": "刚刚", en: "just now" },
  "common.time.secondsAgo": { "zh-CN": "{count} 秒前", en: "{count}s ago" },
  "common.time.minutesAgo": { "zh-CN": "{count} 分钟前", en: "{count} min ago" },
  "common.time.hoursAgo": { "zh-CN": "{count} 小时前", en: "{count}h ago" },
  "common.time.daysAgo": { "zh-CN": "{count} 天前", en: "{count}d ago" },
  "common.imageSummary": {
    "zh-CN": "图片 {width}×{height} · {size}",
    en: "Image {width}×{height} · {size}",
  },

  /* --- 全局 toast（事件层，不属于任何页面） --- */
  "common.toast.initFailed.title": { "zh-CN": "初始化失败", en: "Startup failed" },
  "common.toast.initFailed.description": {
    "zh-CN": "{count} 个接口没有响应：{message}",
    en: "{count} calls did not respond: {message}",
  },
  "common.toast.received.title": { "zh-CN": "收到剪贴板", en: "Clipboard received" },
  "common.toast.sent.title": { "zh-CN": "已发送", en: "Sent" },
  "common.toast.sent.description": {
    "zh-CN": "投递到 {count} 台在线设备",
    en: "Delivered to {count} online devices",
  },
  "common.toast.noOnlineDevices.description": {
    "zh-CN": "内容已留在历史里，等设备上线后可重发。",
    en: "The item is in your history — resend it once a device comes online.",
  },
  "common.toast.error.title": { "zh-CN": "出错了", en: "Something went wrong" },

  /* --- 改名 / 证书：两个平台的措辞一模一样，只留一份 --- */
  "common.toast.renamed": { "zh-CN": "设备名已更新", en: "Device name updated" },
  "common.toast.renamedDescription": {
    "zh-CN": "mDNS 已重新广播，其他设备会看到新名字。",
    en: "mDNS has been re-announced; other devices will see the new name.",
  },
  "common.toast.renameFailed": { "zh-CN": "改名失败", en: "Rename failed" },
  "common.toast.certExported": { "zh-CN": "证书已导出", en: "Certificate exported" },
  "common.toast.certExportedDescription": {
    "zh-CN": "可以把 .pem 文件发给对方，用来人工核对指纹。",
    en: "You can send the .pem file to the other side so they can check the fingerprint by hand.",
  },
  "common.toast.noCertificate": {
    "zh-CN": "没有可导出的证书",
    en: "There is no certificate to export",
  },
} satisfies MessageTable;
