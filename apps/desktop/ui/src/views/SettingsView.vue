<script setup lang="ts">
import { computed, ref, watch } from "vue";

import {
  AppButton,
  AppCard,
  AppIcon,
  AppToggle,
  ConfirmDialog,
  FingerprintBadge,
  MAX_IMAGE_BYTES_OPTIONS,
  StatusPill,
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
 * 桌面设置页：同步行为 + 本机身份/证书 + 危险操作。
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

const identity = computed(() => identityStore.identity);
const mock = isMock();

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

async function saveName(): Promise<void> {
  if (!nameChanged.value) return;
  savingName.value = true;
  try {
    await settingsStore.setDeviceName(draftName.value);
    toast.success("设备名已更新", "mDNS 已重新广播，其他设备会看到新名字。");
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

async function exportCert(): Promise<void> {
  const ok = identityStore.exportCertificate();
  if (ok) {
    toast.success("证书已导出", "可以把 .pem 文件发给对方，用来人工核对指纹。");
  } else {
    toast.error("没有可导出的证书");
  }
}

async function copy(value: string, label: string): Promise<void> {
  const ok = await copyToClipboard(value);
  if (ok) toast.success(`${label}已复制`);
  else toast.error("复制失败");
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
    <header class="cm-page-head">
      <div>
        <h1 class="cm-page-title">设置</h1>
        <p class="cm-page-sub">
          改动会立即生效并写入本机配置。标记
          <AppIcon class="inline-icon" name="zap" :size="12" />
          的选项不需要重启应用。
        </p>
      </div>
      <div class="cm-page-actions">
        <StatusPill v-if="mock" label="MOCK 数据" tone="warn" icon="info" />
        <StatusPill
          :label="settingsStore.saving ? '保存中…' : '配置已就绪'"
          :tone="settingsStore.saving ? 'accent' : 'ok'"
          :pulse="settingsStore.saving"
        />
      </div>
    </header>

    <div class="cols">
      <div class="col">
        <AppCard title="本机名称" icon="monitor" subtitle="会通过 mDNS 广播给同网段的所有设备">
          <div class="field-row">
            <input
              v-model="draftName"
              class="cm-input"
              type="text"
              maxlength="64"
              placeholder="例如：书房的台式机"
              @keydown.enter="saveName"
            />
            <AppButton
              variant="primary"
              icon="check"
              :disabled="!nameChanged"
              :loading="savingName"
              @click="saveName"
            >
              保存
            </AppButton>
          </div>
          <p class="cm-help">
            当前生效：<b>{{ settingsStore.settings?.deviceName ?? statusStore.deviceName }}</b>
          </p>
        </AppCard>

        <AppCard title="同步" icon="refresh" subtitle="控制哪些内容会被自动同步">
          <div class="toggles">
            <AppToggle
              :model-value="settingsStore.settings?.autoSync ?? false"
              label="后台自动同步"
              description="监听本机剪贴板，内容一变化就推送给已信任设备。"
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
              description="图片以原始 PNG 二进制分片传输，不走 Base64。"
              @update:model-value="(v: boolean) => patch({ syncImages: v })"
            />
          </div>

          <div class="field-row mt">
            <label class="cm-field grow">
              <span class="cm-label">图片大小上限</span>
              <select
                class="cm-select"
                :value="settingsStore.settings?.maxImageBytes ?? 0"
                @change="onMaxImageBytes"
              >
                <option
                  v-for="option in MAX_IMAGE_BYTES_OPTIONS"
                  :key="option.value"
                  :value="option.value"
                >
                  {{ option.label }}
                </option>
              </select>
            </label>
            <div class="hint-box">
              超过 <b>{{ formatMaxImageBytes(settingsStore.settings?.maxImageBytes ?? 0) }}</b> 的图片会被跳过，
              并在日志里记一条。
            </div>
          </div>
        </AppCard>

        <AppCard title="桌面行为" icon="zap" subtitle="窗口与开机启动">
          <div class="toggles">
            <AppToggle
              :model-value="settingsStore.settings?.startMinimized ?? false"
              label="启动后最小化到托盘"
              description="开机自启时不弹出窗口，只在托盘里待命。"
              @update:model-value="(v: boolean) => patch({ startMinimized: v })"
            />
            <AppToggle
              :model-value="settingsStore.settings?.launchAtLogin ?? false"
              label="开机自启"
              @update:model-value="(v: boolean) => patch({ launchAtLogin: v })"
            />
          </div>
        </AppCard>
      </div>

      <div class="col">
        <AppCard title="本机身份" icon="key" subtitle="长期保存，卸载重装会重新生成">
          <div v-if="identity" class="identity">
            <FingerprintBadge :fingerprint="identity.fingerprint" label="证书指纹" size="lg" />

            <div class="kv">
              <span class="k">设备 ID</span>
              <span class="v cm-mono">{{ identity.deviceId }}</span>
              <AppButton
                size="sm"
                variant="ghost"
                icon="copy"
                icon-only
                title="复制设备 ID"
                @click="copy(identity.deviceId, '设备 ID ')"
              />
            </div>

            <div class="kv">
              <span class="k">平台</span>
              <span class="v">{{ platformLabel(identity.platform) }}</span>
            </div>

            <div class="kv">
              <span class="k">公钥</span>
              <span class="v cm-mono clamp">{{ identity.publicKey }}</span>
              <AppButton
                size="sm"
                variant="ghost"
                icon="copy"
                icon-only
                title="复制公钥"
                @click="copy(identity.publicKey, '公钥')"
              />
            </div>

            <div class="cert">
              <div class="cert-head">
                <span class="cm-label">设备证书（PEM）</span>
                <AppButton size="sm" icon="download" @click="exportCert">导出证书</AppButton>
              </div>
              <pre class="cm-mono cert-body">{{ identity.certificatePem }}</pre>
              <p class="cm-help">
                对方应该能在自己的设备上看到同一串指纹。指纹不同 = 有人在中间，别继续。
              </p>
            </div>
          </div>
          <div v-else class="loading">正在读取身份信息…</div>
        </AppCard>

        <AppCard title="危险操作" icon="alert" tone="danger" subtitle="不可撤销">
          <div class="danger-row">
            <div>
              <p class="danger-title">清空本机剪贴板历史</p>
              <p class="cm-help">
                删除本机保存的 {{ historyStore.count }} 条记录，不影响已经同步出去的内容。
              </p>
            </div>
            <AppButton
              variant="danger"
              icon="trash"
              :disabled="historyStore.count === 0"
              @click="confirmClear = true"
            >
              清空历史
            </AppButton>
          </div>
        </AppCard>
      </div>
    </div>

    <ConfirmDialog
      :open="confirmClear"
      tone="danger"
      title="清空全部历史？"
      message="本机保存的剪贴板记录会被删除，且无法恢复。"
      confirm-label="清空历史"
      :busy="clearing"
      @cancel="confirmClear = false"
      @confirm="doClearHistory"
    />
  </div>
</template>

<style scoped>
.cols {
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
  gap: var(--space-4);
  align-items: start;
}

.col {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
  min-width: 0;
}

.inline-icon {
  display: inline-block;
  vertical-align: -1px;
  color: var(--warn);
}

.field-row {
  display: flex;
  align-items: flex-end;
  gap: var(--space-3);
}

.field-row.mt {
  margin-top: var(--space-4);
}

.grow {
  flex: 1;
  min-width: 0;
}

.toggles {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.hint-box {
  flex: 1.2;
  padding: 8px 10px;
  border: 1px solid var(--border-soft);
  border-radius: var(--radius-sm);
  background: var(--surface-2);
  color: var(--text-muted);
  font-size: 12px;
  line-height: 1.5;
}

.identity {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.kv {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 10px;
  border: 1px solid var(--border-soft);
  border-radius: var(--radius-sm);
  background: var(--surface-2);
}

.k {
  flex: none;
  width: 62px;
  color: var(--text-dim);
  font-size: 11.5px;
  font-weight: 600;
}

.v {
  flex: 1;
  min-width: 0;
  font-size: 12.5px;
  overflow-wrap: anywhere;
}

.v.clamp {
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}

.cert {
  margin-top: 4px;
  padding: 10px;
  border: 1px solid var(--border-soft);
  border-radius: var(--radius-sm);
  background: var(--surface-2);
}

.cert-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  margin-bottom: 8px;
}

.cert-body {
  max-height: 148px;
  margin: 0 0 8px;
  padding: 9px;
  overflow: auto;
  border-radius: var(--radius-xs);
  background: var(--bg);
  border: 1px solid var(--border);
  color: var(--text-muted);
  font-size: 11px;
  line-height: 1.5;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}

.loading {
  padding: 18px 0;
  color: var(--text-dim);
  font-size: 13px;
}

.danger-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-4);
}

.danger-title {
  font-size: 13.5px;
  font-weight: 600;
}

@media (max-width: 1080px) {
  .cols {
    grid-template-columns: minmax(0, 1fr);
  }
}
</style>
