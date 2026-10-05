<script setup lang="ts">
import { onMounted, ref } from "vue";

import {
  AppCard,
  AppToggle,
  MAX_IMAGE_BYTES_OPTIONS,
  StatusPill,
  androidApi,
  formatMaxImageBytes,
  isMock,
  toMessage,
  useSettingsStore,
  useStatusStore,
  useToast,
  type SettingsView,
} from "@clipmesh/ui-core";

/**
 * Android 设置页：只保留移动端真正需要的项。
 * 桌面专属（托盘、开机自启）不在这里出现。
 *
 * 「本机名称」和「本机身份」在首页的「本机」区块，清空历史在历史页 —— 这里不再重复。
 */
const settingsStore = useSettingsStore();
const statusStore = useStatusStore();
const toast = useToast();

const serviceAvailable = ref(true);
const serviceBusy = ref(false);

const mock = isMock();

onMounted(async () => {
  try {
    await androidApi.isServiceRunning();
  } catch (cause) {
    serviceAvailable.value = false;
    console.info("[clipmesh] Android 专属命令不可用：", toMessage(cause));
  }
});

async function patch(changes: Partial<SettingsView>): Promise<void> {
  try {
    await settingsStore.update(changes);
  } catch (cause) {
    toast.error("设置未保存", toMessage(cause));
  }
}

function onMaxImageBytes(event: Event): void {
  const value = Number((event.target as HTMLSelectElement).value);
  if (Number.isFinite(value)) void patch({ maxImageBytes: value });
}

async function toggleService(value: boolean): Promise<void> {
  serviceBusy.value = true;
  try {
    if (value) {
      // 打开常驻服务时顺带申请通知权限：少了它就没有常驻通知，也收不到远程
      // 剪贴板提醒。被拒绝不算失败 —— 服务照常启动，只是安静地跑。
      const granted = await androidApi.requestNotificationPermission();
      await androidApi.startService();
      await patch({ androidForegroundService: true });

      if (granted) {
        toast.success("前台服务已启动", "通知栏会显示常驻通知与「广播剪贴板」按钮。");
      } else {
        toast.warn("前台服务已启动", "通知权限被拒绝，后台收到内容时不会有提醒。");
      }
      return;
    }

    await androidApi.stopService();
    await patch({ androidForegroundService: false });
    toast.success("前台服务已停止");
  } catch (cause) {
    toast.error("操作失败", toMessage(cause));
  } finally {
    serviceBusy.value = false;
  }
}
</script>

<template>
  <div class="view">
    <AppCard title="同步" icon="refresh">
      <AppToggle
        :model-value="settingsStore.settings?.autoSync ?? false"
        label="后台自动同步"
        description="剪贴板变化时自动推送。"
        @update:model-value="(v: boolean) => patch({ autoSync: v })"
      />
      <AppToggle
        :model-value="settingsStore.settings?.syncText ?? false"
        label="同步文本"
        @update:model-value="(v: boolean) => patch({ syncText: v })"
      />
      <AppToggle
        :model-value="settingsStore.settings?.syncImages ?? false"
        label="同步图片"
        description="移动网络下建议关掉，图片可能很大。"
        @update:model-value="(v: boolean) => patch({ syncImages: v })"
      />

      <label class="cm-field mt">
        <span class="cm-label">图片大小上限</span>
        <select
          class="cm-select"
          :value="settingsStore.settings?.maxImageBytes ?? 0"
          @change="onMaxImageBytes"
        >
          <option v-for="option in MAX_IMAGE_BYTES_OPTIONS" :key="option.value" :value="option.value">
            {{ option.label }}
          </option>
        </select>
        <span class="cm-help">
          当前上限 {{ formatMaxImageBytes(settingsStore.settings?.maxImageBytes ?? 0) }}，超过的图片会被跳过。
        </span>
      </label>
    </AppCard>

    <AppCard v-if="serviceAvailable" title="后台常驻" icon="bell" subtitle="前台服务 + 通知">
      <AppToggle
        :model-value="settingsStore.settings?.androidForegroundService ?? false"
        :disabled="serviceBusy"
        label="常驻前台服务"
        description="保持后台运行，通知栏会显示常驻通知与「广播剪贴板」按钮。"
        @update:model-value="toggleService"
      />
      <p class="cm-help mt-sm">
        打开时会在 Android 13 及以上申请通知权限 —— 没有它就没有常驻通知，也收不到远程剪贴板提醒。
      </p>
    </AppCard>

    <AppCard title="关于" icon="info">
      <div class="about">
        <span>ClipMesh 0.1.0</span>
        <StatusPill
          :label="mock ? '浏览器 MOCK' : 'Tauri 运行时'"
          :tone="mock ? 'warn' : 'ok'"
          size="sm"
        />
      </div>
      <p class="cm-help mt-sm">
        无中心服务器，设备之间直接通过 TLS 通信。当前监听端口
        {{ statusStore.listenPort }}。
      </p>
    </AppCard>
  </div>
</template>

<style scoped>
.view {
  display: flex;
  flex-direction: column;
  gap: 14px;
}

.mt {
  margin-top: 12px;
}

.mt-sm {
  margin-top: 8px;
}

.about {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  font-size: 13.5px;
  font-weight: 600;
}
</style>
