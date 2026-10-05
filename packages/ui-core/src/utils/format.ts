/** 纯格式化函数：不依赖 Vue，方便在 store / 组件 / 测试里随便用。 */

import type { ClipboardItemView, Platform } from "../types";

const PLATFORM_LABELS: Record<Platform, string> = {
  windows: "Windows",
  linux: "Linux",
  macos: "macOS",
  android: "Android",
  unknown: "未知平台",
};

export function platformLabel(platform: Platform): string {
  return PLATFORM_LABELS[platform] ?? PLATFORM_LABELS.unknown;
}

/** 把 `A1B2C3D4...` 规范成 `A1B2 C3D4 ...`，每 4 个字符一组。 */
export function groupFingerprint(fingerprint: string, groupSize = 4): string {
  const clean = fingerprint.replace(/[^0-9a-fA-F]/g, "").toUpperCase();
  if (clean === "") return "";
  const groups: string[] = [];
  for (let i = 0; i < clean.length; i += groupSize) {
    groups.push(clean.slice(i, i + groupSize));
  }
  return groups.join(" ");
}

/** 指纹的前 N 组，用于列表这种空间有限的地方。 */
export function shortFingerprint(fingerprint: string, groups = 2): string {
  return groupFingerprint(fingerprint).split(" ").slice(0, groups).join(" ");
}

export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return "—";
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KiB", "MiB", "GiB"];
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value.toFixed(value >= 10 ? 0 : 1)} ${units[unit]}`;
}

function pad(value: number): string {
  return value < 10 ? `0${value}` : String(value);
}

/** `14:03` 这种当天时刻。 */
export function formatClock(timestamp: number): string {
  const date = new Date(timestamp);
  return `${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

/** `3月5日 14:03` 这种带日期的时刻。 */
export function formatDateTime(timestamp: number): string {
  const date = new Date(timestamp);
  const now = new Date();
  const sameYear = date.getFullYear() === now.getFullYear();
  const head = sameYear
    ? `${date.getMonth() + 1}月${date.getDate()}日`
    : `${date.getFullYear()}/${date.getMonth() + 1}/${date.getDate()}`;
  return `${head} ${formatClock(timestamp)}`;
}

/** 「刚刚 / 3 分钟前 / 2 小时前 / 5 天前」。 */
export function formatRelative(timestamp: number, now = Date.now()): string {
  const diff = now - timestamp;
  if (!Number.isFinite(diff)) return "—";
  if (diff < 5_000) return "刚刚";
  if (diff < 60_000) return `${Math.floor(diff / 1000)} 秒前`;
  if (diff < 3_600_000) return `${Math.floor(diff / 60_000)} 分钟前`;
  if (diff < 86_400_000) return `${Math.floor(diff / 3_600_000)} 小时前`;
  if (diff < 7 * 86_400_000) return `${Math.floor(diff / 86_400_000)} 天前`;
  return formatDateTime(timestamp);
}

/** 长文本截断（用于 toast / 列表预览）。 */
export function truncate(text: string, max = 80): string {
  const flat = text.replace(/\s+/g, " ").trim();
  return flat.length <= max ? flat : `${flat.slice(0, max - 1)}…`;
}

/** 历史条目的一行摘要。 */
export function summarizeItem(item: ClipboardItemView, max = 80): string {
  if (item.kind === "text") return truncate(item.content, max);
  return `图片 ${item.width}×${item.height} · ${formatBytes(item.size)}`;
}

/** UUID 之类的短展示。 */
export function shortId(id: string, head = 8): string {
  return id.length <= head ? id : `${id.slice(0, head)}…`;
}

/** 设置里图片上限的可读形式。 */
export function formatMaxImageBytes(bytes: number): string {
  return bytes >= 1024 * 1024 ? `${Math.round(bytes / (1024 * 1024))} MiB` : formatBytes(bytes);
}
