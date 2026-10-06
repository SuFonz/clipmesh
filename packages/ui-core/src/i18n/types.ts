/**
 * i18n 的类型地基。
 *
 * 刻意不引第三方库：整个界面只需要「两张表 + 一个查表函数」，
 * vue-i18n 带来的体积与配置面在这个规模上不划算。
 */

/** 界面真正渲染用的语言（`system` 解析之后的结果）。 */
export type MessageLocale = "zh-CN" | "en";

/**
 * 一条文案。
 *
 * 两个语言写在**同一个对象字面量**里，所以缺一个就是编译错误 ——
 * 不可能出现「加了中文忘了英文」的半成品。
 */
export interface MessageEntry {
  "zh-CN": string;
  en: string;
}

/** 每个文案文件都实现这个形状；键名用扁平的点分命名（`a.b.c`）。 */
export type MessageTable = Record<string, MessageEntry>;

/** `t()` 的插值参数：文案里写 `{name}`，这里给 `name`。 */
export type MessageParams = Record<string, string | number>;
