<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";

import {
  AppButton,
  AppCard,
  AppToggle,
  ConfirmDialog,
  FingerprintBadge,
  MAX_IMAGE_BYTES_OPTIONS,
  StatusPill,
  androidApi,
  copyToClipboard,
  formatMaxImageBytes,
  isMock,
  platformLabel,
  toMessage,
  useHistoryStore,
  useIdentityStore,
  useSettingsStore,
  useStatusStore,
  useToast,
  type SettingsView,
} from "@clipmesh/ui-core";

/**
 * Android 设置页：只保留移动端真正需要的项。
 * 桌面专属（托盘、开机自启）不在这里出现。
 */
const settingsStore = useSettingsStore();
const identityStore = useIdentityStore();
const statusStore = useStatusStore();
const historyStore = useHistoryStore();
const toast = useToast();

const draftName = ref("");
const savingName = ref(false);
const confirmClear = ref(false);
const clearing = ref(false);
const serviceAvailable = ref(true);
const serviceBusy = ref(false);
const notificationBusy = ref(false);

const mock = isMock();
const identity = computed(() => identityStore.identity);

watch(
  () => settingsStore.settings?.deviceName,
  (name) => {
    if (name !== undefined) draftName.value = name;
  },
  { immediate: true },
);

const nameChanged = computed<boolean>(
  () =>
    draftName.value.trim() !== "" && draftName.value.trim() !== settingsStore.settings?.deviceName,
);

onMounted(async () => {
  try {
    await androidApi.isServiceRunning();
  } catch (cause) {
    serviceAvailable.value = false;
    console.info("[clipmesh] Android 专属命令不可用：", toMessage(cause));
  }
});

async function saveName(): Promise<void> {
  if (!nameChanged.value) return;
  savingName.value = true;
  try {
    await settingsStore.setDeviceName(draftName.value);
    toast.success("设备名已更新");
  } catch (cause) {
    toast.error("改名失败", toMessage(cause));
  } finally {
    savingName.value = false;
  }
}

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
    if (value) await androidApi.startService();
    else await androidApi.stopService();
    await patch({ androidForegroundService: value });
    toast.success(value ? "前台服务已启动" : "前台服务已停止");
  } catch (cause) {
    toast.error("操作失败", toMessage(cause));
  } finally {
    serviceBusy.value = false;
  }
}

async function requestNotification(): Promise<void> {
  notificationBusy.value = true;
  try {
    const granted = await androidApi.requestNotificationPermission();
    toast[granted ? "success" : "warn"](granted ? "通知权限已授予" : "通知权限被拒绝");
  } catch (cause) {
    toast.error("请求失败", toMessage(cause));
  } finally {
    notificationBusy.value = false;
  }
}

async function copyDeviceId(): Promise<void> {
  const id = identity.value?.deviceId ?? "";
  if (id === "") return;
  const ok = await copyToClipboard(id);
  if (ok) toast.success("设备 ID 已复制");
  else toast.error("复制失败");
}

function exportCert(): void {
  if (identityStore.exportCertificate()) toast.success("证书已导出");
  else toast.error("没有可导出的证书");
}

async function doClearHistory(): Promise<void> {
  clearing.value = true;
  try {
    await historyStore.clear();
    confirmClear.value = false;
    toast.success("历史已清空");
  } catch (cause) {
    toast.error("清空失败", toMessage(cause));
  } finally {
    clearing.value = false;
  }
}
</script>

<template>
  <div class="view">
    <AppCard title="本机名称" icon="phone" subtitle="同一网络里的其他设备会看到这个名字">
      <input
        v-model="draftName"
        class="cm-input"
        type="text"
        maxlength="64"
        placeholder="例如：我的 Pixel"
      />
      <AppButton
        class="mt"
        variant="primary"
        block
        icon="check"
        :disabled="!nameChanged"
        :loading="savingName"
        @click="saveName"
      >
        保存名称
      </AppButton>
    </AppCard>

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
      <AppButton class="mt" block icon="bell" :loading="notificationBusy" @click="requestNotification">
        申请通知权限
      </AppButton>
      <p class="cm-help mt-sm">Android 13 及以上需要通知权限，否则收不到远程剪贴板提醒。</p>
    </AppCard>

    <AppCard title="本机身份" icon="shield" subtitle="用于核对配对，不会上传到任何服务器">
      <template v-if="identity">
        <FingerprintBadge :fingerprint="identity.fingerprint" label="证书指纹" />
        <div class="rows">
          <div class="row">
            <span class="k">设备 ID</span>
            <span class="v cm-mono cm-truncate">{{ identity.deviceId }}</span>
          </div>
          <div class="row">
            <span class="k">平台</span>
            <span class="v">{{ platformLabel(identity.platform) }}</span>
          </div>
        </div>
        <div class="actions">
          <AppButton block icon="copy" @click="copyDeviceId">复制设备 ID</AppButton>
          <AppButton block icon="download" @click="exportCert">导出证书</AppButton>
        </div>
      </template>
      <p v-else class="cm-help">正在读取身份信息…</p>
    </AppCard>

    <AppCard title="危险操作" icon="alert" tone="danger">
      <p class="cm-help">清空本机保存的 {{ historyStore.count }} 条剪贴板记录。</p>
      <AppButton
        class="mt"
        variant="danger"
        block
        icon="trash"
        :disabled="historyStore.count === 0"
        @click="confirmClear = true"
      >
        清空历史
      </AppButton>
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

    <ConfirmDialog
      :open="confirmClear"
      tone="danger"
      title="清空全部历史？"
      message="本机保存的剪贴板记录会被删除，无法恢复。"
      confirm-label="清空"
      :busy="clearing"
      @cancel="confirmClear = false"
      @confirm="doClearHistory"
    />
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

.rows {
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin-top: 12px;
}

.row {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 9px 10px;
  border: 1px solid var(--border-soft);
  border-radius: var(--radius-sm);
  background: var(--surface-2);
}

.k {
  flex: none;
  width: 58px;
  color: var(--text-dim);
  font-size: 11.5px;
  font-weight: 600;
}

.v {
  flex: 1;
  min-width: 0;
  font-size: 12.5px;
}

.actions {
  display: flex;
  gap: 8px;
  margin-top: 12px;
}

.actions > * {
  flex: 1;
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
