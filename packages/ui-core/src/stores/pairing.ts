import { defineStore } from "pinia";
import { computed, ref } from "vue";

import { listPairingRequests, requestPairing, respondPairing } from "../api/commands";
import type { PairingPrompt } from "../types";
import { toMessage } from "./status";

/**
 * 等待用户决定的配对请求。`clipmesh://pairing-requests` 是快照事件。
 */
export const usePairingStore = defineStore("pairing", () => {
  const prompts = ref<PairingPrompt[]>([]);
  const loading = ref(false);
  const error = ref<string | null>(null);
  const busyIds = ref<string[]>([]);

  /** 别人请求我们 —— 需要「接受 / 拒绝」。 */
  const incoming = computed<PairingPrompt[]>(() =>
    prompts.value.filter((p) => p.direction === "incoming"),
  );
  /** 我们请求别人 —— 只展示等待状态。 */
  const outgoing = computed<PairingPrompt[]>(() =>
    prompts.value.filter((p) => p.direction === "outgoing"),
  );
  const count = computed<number>(() => prompts.value.length);
  const incomingCount = computed<number>(() => incoming.value.length);

  function setPrompts(next: PairingPrompt[]): void {
    prompts.value = next;
  }

  function isBusy(deviceId: string): boolean {
    return busyIds.value.includes(deviceId);
  }

  function markBusy(deviceId: string, busy: boolean): void {
    busyIds.value = busy
      ? [...busyIds.value.filter((id) => id !== deviceId), deviceId]
      : busyIds.value.filter((id) => id !== deviceId);
  }

  async function refresh(): Promise<void> {
    loading.value = true;
    try {
      setPrompts(await listPairingRequests());
      error.value = null;
    } catch (cause) {
      error.value = toMessage(cause);
      throw cause;
    } finally {
      loading.value = false;
    }
  }

  /** 接受 / 拒绝一个收到的配对请求。 */
  async function respond(deviceId: string, accept: boolean): Promise<void> {
    markBusy(deviceId, true);
    try {
      await respondPairing(deviceId, accept);
      prompts.value = prompts.value.filter((p) => p.deviceId !== deviceId);
      error.value = null;
    } catch (cause) {
      error.value = toMessage(cause);
      throw cause;
    } finally {
      markBusy(deviceId, false);
    }
  }

  /** 主动发起配对（等价于 peers store 的 pair，但保留在配对域里）。 */
  async function request(deviceId: string): Promise<void> {
    markBusy(deviceId, true);
    try {
      await requestPairing(deviceId);
      error.value = null;
    } catch (cause) {
      error.value = toMessage(cause);
      throw cause;
    } finally {
      markBusy(deviceId, false);
    }
  }

  return {
    prompts,
    loading,
    error,
    busyIds,
    incoming,
    outgoing,
    count,
    incomingCount,
    setPrompts,
    isBusy,
    markBusy,
    refresh,
    respond,
    request,
  };
});
