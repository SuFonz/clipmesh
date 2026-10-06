/**
 * 全部界面文案。
 *
 * 按域拆成五张表，这里拼成一张总表。`MessageKey` 从总表推导出来，
 * 所以 `t("setings.tilte")` 这种拼写错误是**编译错误**，不是运行时空白。
 *
 * 加新文案时改对应的那张表即可 —— 不要在这里加分支。
 */

import { android } from "./messages/android";
import { common } from "./messages/common";
import { components } from "./messages/components";
import { desktop } from "./messages/desktop";
import { settings } from "./messages/settings";

export const catalog = {
  ...common,
  ...settings,
  ...components,
  ...desktop,
  ...android,
};

/** 所有合法的文案键。 */
export type MessageKey = keyof typeof catalog;
