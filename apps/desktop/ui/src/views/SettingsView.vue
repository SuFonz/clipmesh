<script setup lang="ts">
import {
  AppCard,
  AppIcon,
  AppToggle,
  MAX_IMAGE_BYTES_OPTIONS,
  StatusPill,
  formatMaxImageBytes,
  isMock,
  toMessage,
  useSettingsStore,
  useToast,
  type SettingsView,
} from "@clipmesh/ui-core";

/**
 * 桌面设置页：只留同步行为和桌面窗口相关的开关。
 * 本机身份 / 改名在首页的「本机」区块，清除剪贴板历史在历史页。
 */
const settingsStore = useSettingsStore();
const toast = useToast();

const mock = isMock();

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

    <div class="cm-grid">
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
  </div>
</template>

<style scoped>
.inline-icon {
  display: inline-block;
  vertical-align: -1px;
  color: var(--warn);
}

.field-row {
  display: flex;
  align-items: flex-end;
  flex-wrap: wrap;
  gap: var(--space-3);
}

.field-row.mt {
  margin-top: var(--space-4);
}

.grow {
  flex: 1 1 200px;
  min-width: 0;
}

.toggles {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.hint-box {
  flex: 1 1 200px;
  padding: 8px 10px;
  border: 1px solid var(--border-soft);
  border-radius: var(--radius-sm);
  background: var(--surface-2);
  color: var(--text-muted);
  font-size: 12px;
  line-height: 1.5;
}
</style>
