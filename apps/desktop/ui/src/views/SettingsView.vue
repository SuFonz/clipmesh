<script setup lang="ts">
import {
  AppCard,
  AppIcon,
  AppToggle,
  LANGUAGE_OPTIONS,
  MAX_IMAGE_BYTES_OPTIONS,
  StatusPill,
  formatMaxImageBytes,
  isMock,
  languageLabel,
  normalizeLanguageSetting,
  t,
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
    toast.error(t("settings.saveFailed"), toMessage(cause));
  }
}

function onMaxImageBytes(event: Event): void {
  const value = Number((event.target as HTMLSelectElement).value);
  if (Number.isFinite(value)) void patch({ maxImageBytes: value });
}

/** 语言只认三种取值，收窄交给 i18n 层（与 Rust 侧同一条规则）。 */
function onLanguage(event: Event): void {
  const value = normalizeLanguageSetting((event.target as HTMLSelectElement).value);
  void patch({ language: value });
}
</script>

<template>
  <div class="view">
    <header class="cm-page-head">
      <div>
        <h1 class="cm-page-title">{{ t("settings.title") }}</h1>
        <p class="cm-page-sub">
          {{ t("settings.subtitle") }}
          <AppIcon class="inline-icon" name="zap" :size="12" />
          {{ t("settings.subtitleZap") }}
        </p>
      </div>
      <div class="cm-page-actions">
        <StatusPill v-if="mock" :label="t('common.mockBadge')" tone="warn" icon="info" />
        <StatusPill
          :label="settingsStore.saving ? t('settings.status.saving') : t('settings.status.ready')"
          :tone="settingsStore.saving ? 'accent' : 'ok'"
          :pulse="settingsStore.saving"
        />
      </div>
    </header>

    <div class="page-stack">
      <AppCard
        :title="t('settings.language.label')"
        icon="settings"
        :subtitle="t('settings.language.help')"
      >
        <select
          class="cm-select lang-select"
          :value="settingsStore.language"
          :aria-label="t('settings.language.label')"
          @change="onLanguage"
        >
          <option v-for="value in LANGUAGE_OPTIONS" :key="value" :value="value">
            {{ languageLabel(value) }}
          </option>
        </select>
      </AppCard>

      <AppCard
        :title="t('settings.sync.title')"
        icon="refresh"
        :subtitle="t('settings.sync.subtitle')"
      >
        <div class="toggles">
          <AppToggle
            :model-value="settingsStore.settings?.autoSync ?? false"
            :label="t('settings.autoSync.label')"
            :description="t('settings.autoSync.description')"
            @update:model-value="(v: boolean) => patch({ autoSync: v })"
          />
          <AppToggle
            :model-value="settingsStore.settings?.syncText ?? false"
            :label="t('settings.syncText.label')"
            @update:model-value="(v: boolean) => patch({ syncText: v })"
          />
          <AppToggle
            :model-value="settingsStore.settings?.syncImages ?? false"
            :label="t('settings.syncImages.label')"
            :description="t('settings.syncImages.description')"
            @update:model-value="(v: boolean) => patch({ syncImages: v })"
          />
        </div>

        <div class="field-row mt">
          <label class="cm-field grow">
            <span class="cm-label">{{ t("settings.maxImageBytes.label") }}</span>
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
            {{ t("settings.maxImageBytes.hintBefore")
            }}<b>{{ formatMaxImageBytes(settingsStore.settings?.maxImageBytes ?? 0) }}</b
            >{{ t("settings.maxImageBytes.hintAfter") }}
          </div>
        </div>
      </AppCard>

      <AppCard
        :title="t('settings.desktop.title')"
        icon="zap"
        :subtitle="t('settings.desktop.subtitle')"
      >
        <div class="toggles">
          <AppToggle
            :model-value="settingsStore.settings?.startMinimized ?? false"
            :label="t('settings.startMinimized.label')"
            :description="t('settings.startMinimized.description')"
            @update:model-value="(v: boolean) => patch({ startMinimized: v })"
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

.lang-select {
  max-width: 260px;
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
