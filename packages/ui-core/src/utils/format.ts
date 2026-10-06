/**
 * 纯格式化函数：不依赖 Vue 组件，store / 组件 / 测试里都能随便用。
 *
 * 与 i18n 的耦合只有一处：这些函数读当前语言的 ref，所以在模板或 `computed`
 * 里调用它们会跟着语言切换重算（在模块顶层调用一次则不会）。
 */

import { locale, t } from "../i18n";
import type { MessageLocale } from "../i18n";
import type { ClipboardItemView, Platform } from "../types";

/** 平台名是专有名词，不翻译；只有「未知」这一档需要文案。 */
const PLATFORM_LABELS: Record<Exclude<Platform, "unknown">, string> = {
  windows: "Windows",
  linux: "Linux",
  macos: "macOS",
  android: "Android",
};

export function platformLabel(platform: Platform): string {
  const known = PLATFORM_LABELS[platform as Exclude<Platform, "unknown">];
  return known ?? t("common.platformUnknown");
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

/** `3月5日` / `Mar 5` 这种「月 + 日」。按语言各建一个，避免每次调用都新建。 */
const MONTH_DAY_FORMATTERS: Record<MessageLocale, Intl.DateTimeFormat> = {
  "zh-CN": new Intl.DateTimeFormat("zh-CN", { month: "long", day: "numeric" }),
  en: new Intl.DateTimeFormat("en", { month: "short", day: "numeric" }),
};

/** `3月5日 14:03` / `Mar 5 14:03` 这种带日期的时刻。 */
export function formatDateTime(timestamp: number): string {
  const date = new Date(timestamp);
  const now = new Date();
  const sameYear = date.getFullYear() === now.getFullYear();
  const head = sameYear
    ? MONTH_DAY_FORMATTERS[locale.value].format(date)
    : `${date.getFullYear()}/${date.getMonth() + 1}/${date.getDate()}`;
  return `${head} ${formatClock(timestamp)}`;
}

/** 「刚刚 / 3 分钟前 / 2 小时前 / 5 天前」。 */
export function formatRelative(timestamp: number, now = Date.now()): string {
  const diff = now - timestamp;
  if (!Number.isFinite(diff)) return "—";
  if (diff < 5_000) return t("common.time.justNow");
  if (diff < 60_000) return t("common.time.secondsAgo", { count: Math.floor(diff / 1000) });
  if (diff < 3_600_000) return t("common.time.minutesAgo", { count: Math.floor(diff / 60_000) });
  if (diff < 86_400_000) return t("common.time.hoursAgo", { count: Math.floor(diff / 3_600_000) });
  if (diff < 7 * 86_400_000) return t("common.time.daysAgo", { count: Math.floor(diff / 86_400_000) });
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
  return t("common.imageSummary", {
    width: item.width,
    height: item.height,
    size: formatBytes(item.size),
  });
}

/** UUID 之类的短展示。 */
export function shortId(id: string, head = 8): string {
  return id.length <= head ? id : `${id.slice(0, head)}…`;
}

/** 设置里图片上限的可读形式。 */
export function formatMaxImageBytes(bytes: number): string {
  return bytes >= 1024 * 1024 ? `${Math.round(bytes / (1024 * 1024))} MiB` : formatBytes(bytes);
}
