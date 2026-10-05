import { defineStore } from "pinia";
import { computed, ref } from "vue";

import { listPeers, requestPairing, unpairDevice } from "../api/commands";
import type { PeerView } from "../types";
import { toMessage, useStatusStore } from "./status";
import { useTrustedStore } from "./trusted";

/**
 * 网络里当前可见的设备。`clipmesh://peers` 是快照事件 —— 收到就整体替换，
 * 前端不做合并（见 `docs/IPC.md` §1）。
 */
export const usePeersStore = defineStore("peers", () => {
  const statusStore = useStatusStore();
  const trustedStore = useTrustedStore();

  const peers = ref<PeerView[]>([]);
  const loading = ref(false);
  const error = ref<string | null>(null);
  /** 正在执行配对/解除的设备 id（用于按钮 loading 态）。 */
  const busyIds = ref<string[]>([]);

  const count = computed<number>(() => peers.value.length);
  const online = computed<PeerView[]>(() => peers.value.filter((p) => p.connected));
  const onlineCount = computed<number>(() => online.value.length);
  /** 发现了但还没建立信任的设备。 */
  const untrusted = computed<PeerView[]>(() => peers.value.filter((p) => !p.trusted));
  const trusted = computed<PeerView[]>(() => peers.value.filter((p) => p.trusted));
  const paired = computed<PeerView[]>(() => peers.value.filter((p) => p.pairing));

  function setPeers(next: PeerView[]): void {
    peers.value = next;
  }

  function byId(deviceId: string): PeerView | undefined {
    return peers.value.find((p) => p.deviceId === deviceId);
  }

  function isBusy(deviceId: string): boolean {
    return busyIds.value.includes(deviceId);
  }

  function markBusy(deviceId: string, busy: boolean): void {
    busyIds.value = busy
      ? [...busyIds.value.filter((id) => id !== deviceId), deviceId]
      : busyIds.value.filter((id) => id !== deviceId);
  }

  /**
   * 设备 id -> 展示名。历史条目、toast 里只有 id，需要在这里翻译成人看的名字。
   */
  function nameOf(deviceId: string): string {
    if (deviceId === "") return "未知设备";
    if (deviceId === statusStore.deviceId) return "本机";
    const peer = byId(deviceId);
    if (peer) return peer.name;
    const known = trustedStore.nameOf(deviceId);
    if (known !== "") return known;
    return `${deviceId.slice(0, 8)}…`;
  }

  async function refresh(): Promise<void> {
    loading.value = true;
    try {
      setPeers(await listPeers());
      error.value = null;
    } catch (cause) {
      error.value = toMessage(cause);
      throw cause;
    } finally {
      loading.value = false;
    }
  }

  /** 主动向已发现设备发起配对。 */
  async function pair(deviceId: string): Promise<void> {
    markBusy(deviceId, true);
    try {
      await requestPairing(deviceId);
      const peer = byId(deviceId);
      if (peer) peer.pairing = true;
      error.value = null;
    } catch (cause) {
      error.value = toMessage(cause);
      throw cause;
    } finally {
      markBusy(deviceId, false);
    }
  }

  /** 移出信任列表并断开。 */
  async function unpair(deviceId: string): Promise<void> {
    markBusy(deviceId, true);
    try {
      await unpairDevice(deviceId);
      const peer = byId(deviceId);
      if (peer) {
        peer.trusted = false;
        peer.connected = false;
        peer.pairing = false;
      }
      error.value = null;
    } catch (cause) {
      error.value = toMessage(cause);
      throw cause;
    } finally {
      markBusy(deviceId, false);
    }
  }

  return {
    peers,
    loading,
    error,
    busyIds,
    count,
    online,
    onlineCount,
    untrusted,
    trusted,
    paired,
    setPeers,
    byId,
    isBusy,
    markBusy,
    nameOf,
    refresh,
    pair,
    unpair,
  };
});
