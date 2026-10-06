import { defineStore } from "pinia";
import { computed, ref } from "vue";

import { getIdentity, setDeviceName as setDeviceNameCommand } from "../api/commands";
import type { IdentityView, Platform } from "../types";
import { downloadTextFile } from "../utils/dom";
import { toMessage } from "./status";

/**
 * 本机身份：设备 id、公钥、证书。用于「我的设备」区块和证书导出。
 */
export const useIdentityStore = defineStore("identity", () => {
  const identity = ref<IdentityView | null>(null);
  const loading = ref(false);
  const error = ref<string | null>(null);

  const deviceId = computed<string>(() => identity.value?.deviceId ?? "");
  const deviceName = computed<string>(() => identity.value?.deviceName ?? "");
  const platform = computed<Platform>(() => identity.value?.platform ?? "unknown");
  const fingerprint = computed<string>(() => identity.value?.fingerprint ?? "");
  const publicKey = computed<string>(() => identity.value?.publicKey ?? "");
  const certificatePem = computed<string>(() => identity.value?.certificatePem ?? "");
  const ready = computed<boolean>(() => identity.value !== null);

  function setIdentity(next: IdentityView): void {
    identity.value = next;
  }

  async function refresh(): Promise<void> {
    loading.value = true;
    try {
      setIdentity(await getIdentity());
      error.value = null;
    } catch (cause) {
      error.value = toMessage(cause);
      throw cause;
    } finally {
      loading.value = false;
    }
  }

  /** 改设备名 —— 后端会重新广播 mDNS，返回新的身份视图。 */
  async function rename(name: string): Promise<IdentityView> {
    const next = await setDeviceNameCommand(name);
    setIdentity(next);
    error.value = null;
    return next;
  }

  /** 导出证书（.pem），供对方核对指纹时人工比对。 */
  function exportCertificate(): boolean {
    const pem = certificatePem.value;
    if (pem === "") return false;
    const safeName = (deviceName.value || "clipmesh-device").replace(/[^\w.-]+/g, "_");
    downloadTextFile(`${safeName}.crt.pem`, pem, "application/x-pem-file");
    return true;
  }

  return {
    identity,
    loading,
    error,
    deviceId,
    deviceName,
    platform,
    fingerprint,
    publicKey,
    certificatePem,
    ready,
    setIdentity,
    refresh,
    rename,
    exportCertificate,
  };
});
