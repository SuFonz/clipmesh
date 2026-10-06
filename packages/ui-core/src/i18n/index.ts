/**
 * 界面语言。
 *
 * 设置里的 `language` 有三种取值：`system` / `zh-CN` / `en`。
 * 这里负责把它解析成真正渲染用的语言，并且把当前语言做成一个 `ref`，
 * 这样切语言不需要刷新页面 —— 读它的模板与 `computed` 会自动重算。
 *
 * 没有语言包加载、没有异步初始化：两张表都在 bundle 里，`t()` 是同步纯查表。
 */

import { computed, ref, type ComputedRef } from "vue";

import type { LanguageSetting } from "../types";
import { catalog, type MessageKey } from "./catalog";
import type { MessageLocale, MessageParams } from "./types";

export type { MessageKey } from "./catalog";
export type { MessageEntry, MessageLocale, MessageParams, MessageTable } from "./types";

/** 语言下拉框的三个选项，顺序就是界面里的顺序。 */
export const LANGUAGE_OPTIONS: readonly LanguageSetting[] = ["system", "zh-CN", "en"];

/**
 * 系统语言，**只解析一次**。
 *
 * 只看 `navigator.language` 的第一项：它是浏览器/系统实际选中的语言，
 * 而 `navigator.languages` 是用户的偏好列表 —— 拿偏好列表的第二、第三项
 * 去猜界面语言会做出用户没要求的选择。
 *
 * `zh` 开头算中文（`zh-CN`、`zh-TW`、`zh-Hans`……），其余一律英文。
 */
function detectSystemLocale(): MessageLocale {
  if (typeof navigator === "undefined") return "en";
  const tag = navigator.language ?? "";
  return tag.toLowerCase().startsWith("zh") ? "zh-CN" : "en";
}

const systemLocale: MessageLocale = detectSystemLocale();

/** 用户选择，原样保存；`system` 表示跟随系统。 */
const languageSetting = ref<LanguageSetting>("system");

/** 解析后的界面语言。 */
export const locale: ComputedRef<MessageLocale> = computed(() =>
  languageSetting.value === "system" ? systemLocale : languageSetting.value,
);

/** 首次运行时的兜底语言（也就是「系统语言」）。 */
export function resolveSystemLocale(): MessageLocale {
  return systemLocale;
}

/**
 * 把任意输入收窄成三种取值之一。
 *
 * 与 Rust 侧 `Language::from_tag` 同一条规则：认不出来的都当 `system`，
 * 绝不让第四个值流进界面。
 */
export function normalizeLanguageSetting(value: unknown): LanguageSetting {
  return value === "zh-CN" || value === "en" ? value : "system";
}

/**
 * 应用一条设置。
 *
 * 由 settings store 在每次拿到设置快照时调用（`get_settings`、`update_settings`、
 * mock 的乐观更新都走那条路），所以这里是**唯一**的同步点。
 */
export function applyLanguageSetting(value: unknown): void {
  languageSetting.value = normalizeLanguageSetting(value);
}

/** 当前的选择值（不是解析结果）。 */
export function currentLanguageSetting(): LanguageSetting {
  return languageSetting.value;
}

function interpolate(template: string, params?: MessageParams): string {
  if (!params) return template;
  return template.replace(/\{(\w+)\}/g, (whole, name: string) => {
    const value = params[name];
    return value === undefined ? whole : String(value);
  });
}

/**
 * 取一条文案。
 *
 * 读了 `locale` 这个 ref，所以在模板 / `computed` 里调用会建立依赖：
 * 语言一变，用到它的地方全部重渲染。
 */
export function t(key: MessageKey, params?: MessageParams): string {
  return interpolate(catalog[key][locale.value], params);
}

const LANGUAGE_LABEL_KEYS: Record<LanguageSetting, MessageKey> = {
  system: "settings.language.system",
  "zh-CN": "settings.language.zh",
  en: "settings.language.en",
};

/** 语言下拉框里某一项的名字（同样是响应式的）。 */
export function languageLabel(value: LanguageSetting): string {
  return t(LANGUAGE_LABEL_KEYS[value]);
}

/** 组件里想少写几个字符时用它。 */
export function useI18n(): {
  t: typeof t;
  locale: ComputedRef<MessageLocale>;
  languageOptions: readonly LanguageSetting[];
  languageLabel: typeof languageLabel;
} {
  return { t, locale, languageOptions: LANGUAGE_OPTIONS, languageLabel };
}
