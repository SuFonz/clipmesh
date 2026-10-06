import { defineStore } from "pinia";
import { computed, ref } from "vue";

import { listTrustedDevices, unpairDevice } from "../api/commands";
import type { TrustedDeviceView } from "../types";
import { toMessage } from "./status";

/**
 * 已信任设备列表。`clipmesh://trusted` 是快照事件 —— 收到就整体替换。
 */
export const useTrustedStore = defineStore("trusted", () => {
  const devices = ref<TrustedDeviceView[]>([]);
  const loading = ref(false);
  const error = ref<string | null>(null);
  const busyId = ref<string | null>(null);

  const count = computed<number>(() => devices.value.length);
  const online = computed<TrustedDeviceView[]>(() => devices.value.filter((d) => d.online));
  const onlineCount = computed<number>(() => online.value.length);

  function setDevices(next: TrustedDeviceView[]): void {
    devices.value = next;
  }

  function byId(deviceId: string): TrustedDeviceView | undefined {
    return devices.value.find((d) => d.deviceId === deviceId);
  }

  function isTrusted(deviceId: string): boolean {
    return devices.value.some((d) => d.deviceId === deviceId);
  }

  function nameOf(deviceId: string): string {
    return byId(deviceId)?.name ?? "";
  }

  async function refresh(): Promise<void> {
    loading.value = true;
    try {
      setDevices(await listTrustedDevices());
      error.value = null;
    } catch (cause) {
      error.value = toMessage(cause);
      throw cause;
    } finally {
      loading.value = false;
    }
  }

  /** 移出信任列表并断开连接。 */
  async function unpair(deviceId: string): Promise<void> {
    busyId.value = deviceId;
    try {
      await unpairDevice(deviceId);
      devices.value = devices.value.filter((d) => d.deviceId !== deviceId);
      error.value = null;
    } catch (cause) {
      error.value = toMessage(cause);
      throw cause;
    } finally {
      busyId.value = null;
    }
  }

  return {
    devices,
    loading,
    error,
    busyId,
    count,
    online,
    onlineCount,
    setDevices,
    byId,
    isTrusted,
    nameOf,
    refresh,
    unpair,
  };
});
