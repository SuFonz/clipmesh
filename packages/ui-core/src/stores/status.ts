import { defineStore } from "pinia";
import { computed, ref } from "vue";

import { clearError as clearErrorCommand, getStatus, startEngine, stopEngine } from "../api/commands";
import type { Platform, StatusView } from "../types";

/**
 * 本机运行状态。数据来源：
 *  - 启动时一次 `get_status`
 *  - 之后全部来自 `clipmesh://status` 快照
 */
export const useStatusStore = defineStore("status", () => {
  const status = ref<StatusView | null>(null);
  const loading = ref(false);
  const error = ref<string | null>(null);

  const running = computed<boolean>(() => status.value?.running ?? false);
  const autoSync = computed<boolean>(() => status.value?.autoSync ?? false);
  const deviceId = computed<string>(() => status.value?.deviceId ?? "");
  const deviceName = computed<string>(() => status.value?.deviceName ?? "本机");
  const platform = computed<Platform>(() => status.value?.platform ?? "unknown");
  const fingerprint = computed<string>(() => status.value?.fingerprint ?? "");
  const listenPort = computed<number>(() => status.value?.listenPort ?? 0);
  const connectedPeers = computed<number>(() => status.value?.connectedPeers ?? 0);
  const trustedPeers = computed<number>(() => status.value?.trustedPeers ?? 0);
  const lastError = computed<string | null>(() => status.value?.lastError ?? null);
  const ready = computed<boolean>(() => status.value !== null);

  /** 事件回调 / 命令返回值都用它写状态（事件是快照，直接替换）。 */
  function setStatus(next: StatusView): void {
    status.value = next;
    if (next.lastError === null) error.value = null;
  }

  function setError(message: string | null): void {
    error.value = message;
  }

  async function refresh(): Promise<void> {
    loading.value = true;
    try {
      setStatus(await getStatus());
      error.value = null;
    } catch (cause) {
      error.value = toMessage(cause);
      throw cause;
    } finally {
      loading.value = false;
    }
  }

  async function start(): Promise<void> {
    setStatus(await startEngine());
  }

  async function stop(): Promise<void> {
    setStatus(await stopEngine());
  }

  /**
   * 关掉当前这条错误。
   *
   * 必须让后端把它忘掉，而不是只清前端：`lastError` 是状态快照里的字段，
   * 下一条 `clipmesh://status` 会原样带回来，横幅就又出现了。
   * 本地那条 `error`（启动填充失败之类的、后端不知道的错）一起清掉，
   * 两个通道不会互相打架。
   */
  async function clearError(): Promise<void> {
    error.value = null;
    setStatus(await clearErrorCommand());
  }

  return {
    status,
    loading,
    error,
    running,
    autoSync,
    deviceId,
    deviceName,
    platform,
    fingerprint,
    listenPort,
    connectedPeers,
    trustedPeers,
    lastError,
    ready,
    setStatus,
    setError,
    refresh,
    start,
    stop,
    clearError,
  };
});

/** Rust 侧的错误是字符串（`CoreError` 的 Display），这里做个兜底。 */
export function toMessage(cause: unknown): string {
  if (typeof cause === "string") return cause;
  if (cause instanceof Error) return cause.message;
  return "未知错误";
}
