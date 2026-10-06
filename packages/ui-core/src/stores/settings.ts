import { defineStore } from "pinia";
import { computed, ref } from "vue";

import { getSettings, updateSettings } from "../api/commands";
import { applyLanguageSetting } from "../i18n";
import type { LanguageSetting, SettingsView } from "../types";
import { useIdentityStore } from "./identity";
import { toMessage, useStatusStore } from "./status";

/** 图片上限的候选值，两个 app 的设置页共用，避免各写一套。 */
export const MAX_IMAGE_BYTES_OPTIONS: ReadonlyArray<{ value: number; label: string }> = [
  { value: 1024 * 1024, label: "1 MiB" },
  { value: 2 * 1024 * 1024, label: "2 MiB" },
  { value: 8 * 1024 * 1024, label: "8 MiB" },
  { value: 32 * 1024 * 1024, label: "32 MiB" },
];

/**
 * 用户设置。`update_settings` 立即生效并持久化；这里做乐观更新 + 失败回滚，
 * 免得每次拨开关都要等一个 IPC 往返。
 */
export const useSettingsStore = defineStore("settings", () => {
  const statusStore = useStatusStore();
  const identityStore = useIdentityStore();

  const settings = ref<SettingsView | null>(null);
  const loading = ref(false);
  const saving = ref(false);
  const error = ref<string | null>(null);

  const ready = computed<boolean>(() => settings.value !== null);
  const autoSync = computed<boolean>(() => settings.value?.autoSync ?? false);
  const syncText = computed<boolean>(() => settings.value?.syncText ?? false);
  const syncImages = computed<boolean>(() => settings.value?.syncImages ?? false);
  const maxImageBytes = computed<number>(() => settings.value?.maxImageBytes ?? 0);
  const startMinimized = computed<boolean>(() => settings.value?.startMinimized ?? false);
  const androidForegroundService = computed<boolean>(
    () => settings.value?.androidForegroundService ?? false,
  );
  const language = computed<LanguageSetting>(() => settings.value?.language ?? "system");

  /**
   * 写本地设置视图。
   *
   * 语言是 i18n 层的全局 ref（不是 Pinia 状态），所以每次拿到设置快照都要
   * 往那边同步一次 —— 这里是**唯一**的同步点，`refresh` 与 `update` 都经过它。
   */
  function setSettings(next: SettingsView): void {
    settings.value = next;
    applyLanguageSetting(next.language);
  }

  async function refresh(): Promise<void> {
    loading.value = true;
    try {
      setSettings(await getSettings());
      error.value = null;
    } catch (cause) {
      error.value = toMessage(cause);
      throw cause;
    } finally {
      loading.value = false;
    }
  }

  /** 局部更新：乐观写本地，失败回滚。 */
  async function update(patch: Partial<SettingsView>): Promise<void> {
    const previous = settings.value;
    if (previous) {
      const optimistic = { ...previous, ...patch };
      settings.value = optimistic;
      // 刚在下拉框里选的语言要立刻生效，不能等一个 IPC 往返
      if (patch.language !== undefined) applyLanguageSetting(optimistic.language);
    }
    saving.value = true;
    try {
      setSettings(await updateSettings(patch));
      // 设备名同时影响状态栏与身份卡片，顺手同步一下本地视图
      if (patch.deviceName !== undefined) {
        const local = statusStore.status;
        if (local) statusStore.setStatus({ ...local, deviceName: patch.deviceName });
        if (identityStore.identity) {
          identityStore.setIdentity({ ...identityStore.identity, deviceName: patch.deviceName });
        }
      }
      error.value = null;
    } catch (cause) {
      if (previous) {
        settings.value = previous;
        applyLanguageSetting(previous.language);
      }
      error.value = toMessage(cause);
      throw cause;
    } finally {
      saving.value = false;
    }
  }

  /** 改设备名（走 `set_device_name`，后端会重新广播 mDNS）。 */
  async function setDeviceName(name: string): Promise<void> {
    const trimmed = name.trim();
    if (trimmed === "") return;
    saving.value = true;
    try {
      const next = await identityStore.rename(trimmed);
      if (settings.value) settings.value = { ...settings.value, deviceName: next.deviceName };
      const local = statusStore.status;
      if (local) statusStore.setStatus({ ...local, deviceName: next.deviceName });
      error.value = null;
    } catch (cause) {
      error.value = toMessage(cause);
      throw cause;
    } finally {
      saving.value = false;
    }
  }

  return {
    settings,
    loading,
    saving,
    error,
    ready,
    autoSync,
    syncText,
    syncImages,
    maxImageBytes,
    startMinimized,
    androidForegroundService,
    language,
    setSettings,
    refresh,
    update,
    setDeviceName,
  };
});
