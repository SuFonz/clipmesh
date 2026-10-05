import { onBeforeUnmount, onMounted } from "vue";

import { listenEvent } from "../api/transport";
import { useHistoryStore } from "../stores/history";
import { useIdentityStore } from "../stores/identity";
import { usePairingStore } from "../stores/pairing";
import { usePeersStore } from "../stores/peers";
import { useSettingsStore } from "../stores/settings";
import { toMessage, useStatusStore } from "../stores/status";
import { useTrustedStore } from "../stores/trusted";
import type { UnlistenFn } from "../types";
import { summarizeItem } from "../utils/format";
import { useToast } from "./useToast";

export interface CoreEventsHandle {
  /** 手动退订（正常路径是组件卸载时自动调用）。 */
  dispose: () => void;
  /** 初始填充完成。 */
  ready: Promise<void>;
}

/**
 * 在 `App.vue` 里挂**一次**即可。
 *
 * 做两件事：
 *  1. 启动时用 `list_*` 命令做一次初始填充（`docs/IPC.md` §1）；
 *  2. 订阅全部 `clipmesh://` 事件并写进对应的 Pinia store。
 *
 * 事件是**快照**不是增量：收到 `clipmesh://peers` 就整体替换 peers 列表。
 */
export function useCoreEvents(): CoreEventsHandle {
  const statusStore = useStatusStore();
  const peersStore = usePeersStore();
  const trustedStore = useTrustedStore();
  const pairingStore = usePairingStore();
  const historyStore = useHistoryStore();
  const settingsStore = useSettingsStore();
  const identityStore = useIdentityStore();
  const toast = useToast();

  const unlisteners: UnlistenFn[] = [];
  let disposed = false;

  /** 启动时的初始填充：并行拉一遍，任何一个失败都只报一次错。 */
  async function bootstrap(): Promise<void> {
    const results = await Promise.allSettled([
      statusStore.refresh(),
      peersStore.refresh(),
      trustedStore.refresh(),
      pairingStore.refresh(),
      historyStore.refresh(),
      settingsStore.refresh(),
      identityStore.refresh(),
    ]);
    const failed = results.filter((r) => r.status === "rejected");
    if (failed.length > 0) {
      const first = failed[0];
      const message = first && first.status === "rejected" ? toMessage(first.reason) : "未知错误";
      statusStore.setError(message);
      toast.error("初始化失败", `${failed.length} 个接口没有响应：${message}`);
    }
  }

  async function subscribe(): Promise<void> {
    const subs = await Promise.all([
      listenEvent("clipmesh://status", (payload) => {
        statusStore.setStatus(payload);
      }),
      listenEvent("clipmesh://peers", (payload) => {
        peersStore.setPeers(payload);
      }),
      listenEvent("clipmesh://trusted", (payload) => {
        trustedStore.setDevices(payload);
      }),
      listenEvent("clipmesh://pairing-requests", (payload) => {
        pairingStore.setPrompts(payload);
      }),
      listenEvent("clipmesh://history", (payload) => {
        historyStore.setItems(payload);
      }),
      listenEvent("clipmesh://clipboard-received", (payload) => {
        historyStore.prepend(payload);
        toast.info("收到剪贴板", summarizeItem(payload, 64));
      }),
      listenEvent("clipmesh://clipboard-sent", (payload) => {
        const { delivered } = payload;
        if (delivered > 0) {
          toast.success("已发送", `投递到 ${delivered} 台在线设备`);
        } else {
          toast.warn("没有在线设备", "内容已留在历史里，等设备上线后可重发。");
        }
      }),
      listenEvent("clipmesh://error", (message) => {
        statusStore.setError(message);
        toast.error("出错了", message);
      }),
    ]);

    if (disposed) {
      // 订阅过程中组件已经卸载了，立刻退订，别泄漏监听器
      for (const unlisten of subs) unlisten();
      return;
    }
    unlisteners.push(...subs);
  }

  const ready: Promise<void> = (async () => {
    await bootstrap();
    await subscribe();
  })();

  onMounted(() => {
    void ready;
  });

  onBeforeUnmount(() => {
    disposed = true;
    for (const unlisten of unlisteners) unlisten();
    unlisteners.length = 0;
  });

  return {
    ready,
    dispose: (): void => {
      disposed = true;
      for (const unlisten of unlisteners) unlisten();
      unlisteners.length = 0;
    },
  };
}
