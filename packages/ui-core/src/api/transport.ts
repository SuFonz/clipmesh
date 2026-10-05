/**
 * IPC 传输层：**唯一**决定「真后端」还是「浏览器 mock」的地方。
 *
 * - 跑在 Tauri 窗口里（`window.__TAURI_INTERNALS__` 存在）→ 动态加载
 *   `@tauri-apps/api` 并调用真实的 `invoke` / `listen`。
 * - 跑在普通浏览器里（`npm --prefix apps/desktop/ui run dev` 直接打开）→
 *   回落到 `./mock.ts` 的内存实现，UI 完全可用、可交互。
 *
 * 之所以用动态 `import()`：在没有 Tauri 的环境里，`@tauri-apps/api` 的模块
 * 根本不会被求值，mock 路径是纯净的；同时真实路径的代码仍然会被 vite 单独
 * 打成一个 chunk，按需加载。
 */

import type {
  Args,
  CommandName,
  EventName,
  EventPayload,
  Result,
  UnlistenFn,
} from "../types";
import { configureMock, mockInvoke, mockListen, resetMock, stopMock } from "./mock";

/** 当前是否运行在 Tauri WebView 里。 */
export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/** 当前是否使用浏览器 mock 后端（UI 里会显示一个 MOCK 角标）。 */
export function isMock(): boolean {
  return !isTauri();
}

/**
 * 调用一个后端命令。
 *
 * @example
 * const status = await call("get_status");
 * const res = await call("send_text", { content: "hello" });
 */
export async function call<K extends CommandName>(name: K, args?: Args<K>): Promise<Result<K>> {
  if (isTauri()) {
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<Result<K>>(name, args as Record<string, unknown> | undefined);
  }
  return mockInvoke(name, args);
}

/**
 * 订阅一个 `clipmesh://` 事件。
 *
 * 事件是**快照**而不是增量：收到什么就整体替换本地状态。
 * 返回的 `unlisten` 在组件卸载时调用。
 */
export async function listenEvent<K extends EventName>(
  name: K,
  handler: (payload: EventPayload<K>) => void,
): Promise<UnlistenFn> {
  if (isTauri()) {
    const { listen } = await import("@tauri-apps/api/event");
    const unlisten = await listen<EventPayload<K>>(name, (event) => {
      handler(event.payload);
    });
    return unlisten;
  }
  return mockListen(name, handler);
}

export { configureMock, resetMock, stopMock };
