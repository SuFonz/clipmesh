import { ref, type Ref } from "vue";

import { isTauri } from "../api/transport";
import { toMessage } from "../stores/status";

/**
 * 应用版本号。
 *
 * **唯一来源**是 Tauri 的包信息（`tauri.conf.json` 的 `version` 字段），运行时用
 * `@tauri-apps/api/app` 的 `getVersion()` 读回来。两个平台的 capability 都是
 * `core:default`，它展开后包含 `core:app:default` → `allow-version`，所以这个命令
 * 已经授权、不用改构建配置；前端因此不再自己维护一份版本号常量。
 *
 * 浏览器 mock 路径没有 Tauri 宿主（`isTauri()` 为 false），保持 `null`，由界面显示
 * 占位符。这里的动态 `import()` 与 `api/transport.ts` 是同一写法：mock 路径根本
 * 不会去加载 `@tauri-apps/api`。
 *
 * 全局共享一个 ref：两端的「关于」读的是同一份，`getVersion()` 只走一次 IPC。
 */
const version = ref<string | null>(null);
let pending: Promise<void> | null = null;

function load(): Promise<void> {
  if (!isTauri()) return Promise.resolve();
  pending ??= import("@tauri-apps/api/app")
    .then(({ getVersion }) => getVersion())
    .then((value) => {
      version.value = value;
    })
    .catch((cause: unknown) => {
      // 意外情况（命令被 capability 挡掉、IPC 出错）：记一条日志，界面照常显示占位符。
      console.info("[clipmesh] 读不到应用版本号：", toMessage(cause));
    });
  return pending;
}

/** 读一次应用版本号；读不到（浏览器 mock）时是 `null`。 */
export function useAppVersion(): Ref<string | null> {
  void load();
  return version;
}
